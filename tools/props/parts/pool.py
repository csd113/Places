"""Pool prop parts.

Goal 5's Pool furniture, curtains, ladder and guardrail modules.  Each entry is
``{"core:<id>": build_function}`` exactly like the other part modules; the
catalog is the authoritative list of ids and sizes.

The set is one family: a clean, relatively new, sterile institutional pool.
Everything is pale -- white resin furniture, chrome ladder, dull-silver
guardrails and pale privacy curtains -- and the wear is deliberately light (a
few faint scuffs, no rust, no mould) because the Pool is empty, not abandoned.
Colours come from :mod:`palette`; the catalogue ``size`` is the authoritative
bounding box and the origin is the floor-contact centre.

Module conventions:

* curtain and guardrail modules are composed on a 0.6 m bay grid so a run can
  be assembled from straight / end / corner pieces in the level;
* the three curtain modules share one construction (25 mm posts, a top rail and
  a hanging folded panel) so a run reads as one object;
* the guardrail modules share the same post and rail stock, the base plates are
  what gives the 8 cm-deep catalogue box its depth, and the straight section's
  posts sit at +/-0.98 m so two sections join post-to-post;
* +Z is each module's front: the ladder's handrails curve towards +Z (over the
  deck edge) and the guardrail/curtain faces are symmetric about it.
"""

from __future__ import annotations

import math

import palette
from mesh import FACE_KEYS, PropBuilder

# ---------------------------------------------------------------- budgets
#
# Pack budget: 500 preferred, 800 review, 1500 hard (tools/props/README.md).
# Each value is the target this module designs to, not a soft hint.

TARGETS = {
    "core:pool_table": 160,
    "core:pool_chair": 190,
    "core:pool_ladder": 240,
    "core:pool_curtain_straight": 90,
    "core:pool_curtain_end": 60,
    "core:pool_curtain_corner": 100,
    "core:pool_guardrail_straight": 160,
    "core:pool_guardrail_end": 130,
    "core:pool_guardrail_corner": 220,
}

# ------------------------------------------------------------------ palette

RESIN = (234, 231, 223)          # the resin's albedo, a touch above the dingy
RESIN_TINT = palette.hex_to_rgb(palette.PLASTIC_WHITE)  # palette white vertex tint
CLOTH = palette.mix(
    palette.hex_to_rgb(palette.PLASTIC_WHITE),
    palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL),
    0.16,
)
CLOTH_TINT = palette.mix(CLOTH, palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.35)
CHROME = palette.hex_to_rgb(palette.CHROME)
CHROME_TINT = palette.mix(CHROME, palette.hex_to_rgb(palette.WALL_CREAM), 0.30)
SILVER = palette.hex_to_rgb(palette.METAL_LIGHT)
SILVER_TINT = palette.mix(SILVER, palette.hex_to_rgb(palette.WALL_CREAM), 0.28)


# ------------------------------------------------------------------ helpers


def _box(p: PropBuilder, center, size, uv, color, hidden=("-y",), colors=None, rotation=None) -> None:
    """A box that skips never-visible faces (each hidden face is 2 triangles)."""
    faces = dict(uv) if isinstance(uv, dict) else {key: uv for key in FACE_KEYS}
    for key in hidden:
        faces[key] = None
    p.box(center, size, uv=faces, color=color, colors=colors, rotation=rotation)


def _paint_resin(tex, region: str, base, seed: int, wear: float = 0.5) -> None:
    """Clean moulded resin: flat albedo, a faint moulding sheen, light scuffs."""
    tex.fill(region, base, jitter=5, seed=seed)
    tex.noise(region, amount=3, freq=5, seed=seed + 1)
    tex.grain(region, palette.shade(base, 0.90), seed=seed + 2, density=0.20, alpha=24)
    tex.grain(region, palette.shade(base, 1.05), seed=seed + 3, density=0.14, alpha=16)
    tex.spots(
        region,
        palette.hex_to_rgb(palette.GRIME),
        count=max(1, int(round(2 * wear))),
        seed=seed + 4,
        radius=2,
        alpha=14,
    )
    tex.border(region, palette.shade(base, 0.88), width=1, alpha=40)


def _paint_cloth(tex, region: str, base, seed: int) -> None:
    """Pale commercial curtain cloth: faint weave, a header and a bottom hem."""
    tex.fill(region, base, jitter=5, seed=seed)
    tex.noise(region, amount=4, freq=4, seed=seed + 1)
    tex.grain(region, palette.shade(base, 0.90), seed=seed + 2, density=0.22, alpha=22)
    tex.grain(region, palette.shade(base, 1.06), seed=seed + 3, density=0.16, alpha=16)
    # The region's small-V end is the panel top (the quads map v1 to the hem).
    tex.band(region, palette.shade(base, 0.88), 0.0, 0.05, alpha=80)
    tex.band(region, palette.shade(base, 0.90), 0.90, 0.955, alpha=70)
    tex.band(region, palette.shade(base, 0.78), 0.955, 1.0, alpha=80)
    tex.border(region, palette.shade(base, 0.86), width=1, alpha=30)


def _paint_metal(tex, region: str, base, seed: int, warm: bool = False) -> None:
    """Dull round metal stock: a soft lengthwise gradient, no gloss."""
    tex.gradient(region, palette.shade(base, 1.10), palette.shade(base, 0.88), jitter=4, seed=seed)
    tex.grain(region, palette.shade(base, 0.82), seed=seed + 1, density=0.30, alpha=28)
    tex.grain(region, palette.shade(base, 1.12), seed=seed + 2, density=0.18, alpha=18)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=2, seed=seed + 3, radius=1, alpha=12)
    if warm:
        tex.spots(region, palette.hex_to_rgb(palette.RUST), count=1, seed=seed + 4, radius=1, alpha=10)
    tex.border(region, palette.shade(base, 0.86), width=1, alpha=30)


def _hanging_panel(p: PropBuilder, xs, zs, top: float, bottom: float, uv, color) -> None:
    """A folded curtain panel: a double-sided zigzag ribbon.

    Each fold is one flat quad; the corners alternate in Z so the panel reads
    as a shallow accordion, and adjacent folds bake slightly different shade
    multipliers exactly the way a real pleat catches the light.  The quads are
    emitted twice (reversed winding) so the panel is solid from both sides
    without paying for a closed shell.
    """
    folds = len(xs) - 1
    for index in range(folds):
        x0, x1 = xs[index], xs[index + 1]
        z0, z1 = zs[index], zs[index + 1]
        u0 = uv[0] + (uv[2] - uv[0]) * (index / folds)
        u1 = uv[0] + (uv[2] - uv[0]) * ((index + 1) / folds)
        rect = (u0, uv[1], u1, uv[3])
        mult = 1.0 if index % 2 == 0 else 0.84
        front = (
            (x0, bottom, z0),
            (x1, bottom, z1),
            (x1, top, z1),
            (x0, top, z0),
        )
        back = (front[1], front[0], front[3], front[2])
        for corners in (front, back):
            p.mesh.quad(*corners, uv=rect, color=color, shade_mult=mult, ao=1.0)


def _curtain_post(p: PropBuilder, x: float, z: float, post_uv, color) -> None:
    """One 25 mm post, floor to the 2.6 m catalogue top."""
    p.cylinder((x, 0.0, z), 0.0125, 2.6, segments=6, side_uv=post_uv, cap_uv=post_uv,
               color=color, bottom=False)


def _curtain_rail(p: PropBuilder, start, axis: str, length: float, rail_uv, color) -> None:
    """The top rail: a 28 mm tube just under the post tops."""
    p.cylinder(start, 0.014, length, axis=axis, segments=6, side_uv=rail_uv, cap_uv=rail_uv,
               color=color, bottom=True)


def _guardrail_post(p: PropBuilder, x: float, z: float, post_uv, color) -> None:
    """One 40 mm guardrail post: floor to 1.05 m, through the top rail."""
    p.cylinder((x, 0.01, z), 0.02, 1.04, segments=6, side_uv=post_uv, cap_uv=post_uv,
               color=color, bottom=False)


def _guardrail_plate(p: PropBuilder, x: float, z: float, plate_uv, color) -> None:
    """A bolted base plate; it is also what fills the catalogue's 8 cm depth."""
    _box(p, (x, 0.008, z), (0.09, 0.016, 0.08), plate_uv, color, hidden=("-y",))


def _guardrail_rail(p: PropBuilder, start, axis: str, length: float, y: float, radius: float,
                    rail_uv, color) -> None:
    p.cylinder((start[0], y, start[1]), radius, length, axis=axis, segments=8,
               side_uv=rail_uv, cap_uv=rail_uv, color=color, bottom=True)


# -------------------------------------------------------------------- table


def build_pool_table(p: PropBuilder) -> None:
    """White resin patio table: square top with a shallow lip, four tapered
    legs and a low cross-brace.  Clean and new."""
    size = p.size  # [0.8, 0.74, 0.8]
    tex = p.set_texture(64, seed=211)
    tex.auto("top", "rim", "leg", "brace")

    _paint_resin(tex, "top", RESIN, 301, wear=0.3)
    _paint_resin(tex, "rim", palette.shade(RESIN, 0.97), 307, wear=0.5)
    _paint_resin(tex, "leg", palette.shade(RESIN, 0.95), 311, wear=0.8)
    _paint_resin(tex, "brace", palette.shade(RESIN, 0.92), 317, wear=0.9)

    top_uv = tex.uv("top")
    rim_uv = tex.uv("rim")
    leg_uv = tex.uv("leg")
    brace_uv = tex.uv("brace")

    top_y = size[1]                      # 0.74
    lip_h = 0.024
    lip_w = 0.05
    slab_h = 0.036
    # The tray floor sits one lip below the catalogue top; the two short rim
    # bars do not reach the tray centre, so nothing is coplanar.
    _box(
        p,
        (0.0, top_y - lip_h - slab_h * 0.5, 0.0),
        (size[0], slab_h, size[2]),
        {"+y": top_uv, "-y": None, "+x": rim_uv, "-x": rim_uv, "+z": rim_uv, "-z": rim_uv},
        RESIN_TINT,
        colors={"+y": palette.shade(RESIN_TINT, 1.03)},
    )
    rim_length = size[0] - 2.0 * lip_w
    for sz in (-1.0, 1.0):
        _box(p, (0.0, top_y - lip_h * 0.5, sz * (size[2] * 0.5 - lip_w * 0.5)),
             (size[0], lip_h, lip_w), rim_uv, palette.shade(RESIN_TINT, 1.02))
    for sx in (-1.0, 1.0):
        _box(p, (sx * (size[0] * 0.5 - lip_w * 0.5), top_y - lip_h * 0.5, 0.0),
             (lip_w, lip_h, rim_length), rim_uv, palette.shade(RESIN_TINT, 1.02))

    # Four tapered legs; their tops sink into the tray floor.
    leg_h = top_y - lip_h - slab_h + 0.01
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.cylinder((sx * 0.33, 0.0, sz * 0.33), 0.032, leg_h, segments=6, taper=0.78,
                       side_uv=leg_uv, cap_uv=leg_uv, color=palette.shade(RESIN_TINT, 0.97))

    # The low cross-brace ties all four legs together.
    brace_y = 0.15
    _box(p, (0.0, brace_y, 0.0), (0.66, 0.045, 0.03), brace_uv, palette.shade(RESIN_TINT, 0.95))
    _box(p, (0.0, brace_y, 0.0), (0.03, 0.045, 0.66), brace_uv, palette.shade(RESIN_TINT, 0.95))
    p.add_note("resin tray top with lip; four tapered legs; one low cross-brace")


# -------------------------------------------------------------------- chair


def build_pool_chair(p: PropBuilder) -> None:
    """White resin patio chair, the table's sibling: same stock, same colours,
    a 42 cm seat and a slatted back raked 9 degrees to the 0.85 m top."""
    size = p.size  # [0.52, 0.85, 0.55]
    tex = p.set_texture(64, seed=223)
    tex.auto("seat", "frame", "leg", "slat")

    _paint_resin(tex, "seat", RESIN, 331, wear=0.4)
    _paint_resin(tex, "frame", palette.shade(RESIN, 0.96), 337, wear=0.7)
    _paint_resin(tex, "leg", palette.shade(RESIN, 0.94), 341, wear=0.9)
    _paint_resin(tex, "slat", palette.shade(RESIN, 0.99), 347, wear=0.3)

    seat_uv = tex.uv("seat")
    frame_uv = tex.uv("frame")
    leg_uv = tex.uv("leg")
    slat_uv = tex.uv("slat")

    seat_top = 0.42
    seat_h = 0.045
    seat_w, seat_d, seat_z = 0.48, 0.50, 0.02
    _box(p, (0.0, seat_top - seat_h * 0.5, seat_z), (seat_w, seat_h, seat_d), seat_uv,
         RESIN_TINT, colors={"+y": palette.shade(RESIN_TINT, 1.04)})

    # Four legs, splayed a touch by their stations rather than their angle.
    for sx in (-1.0, 1.0):
        for sz, station in ((-1.0, -0.17), (1.0, 0.21)):
            p.cylinder((sx * 0.19, 0.0, station), 0.026, seat_top - 0.02, segments=6, taper=0.80,
                       side_uv=leg_uv, cap_uv=leg_uv, color=palette.shade(RESIN_TINT, 0.97))
    # Side rails under the seat tie the legs together.
    for sx in (-1.0, 1.0):
        _box(p, (sx * 0.19, 0.365, 0.02), (0.03, 0.045, 0.40), frame_uv,
             palette.shade(RESIN_TINT, 0.95))

    # The back is a raked frame: two stiles, three slats and a top rail, all on
    # one 9 degree line so it reads as a single moulding.
    rake = 9.0
    hinge_y = seat_top
    hinge_z = -0.20

    def back_z(y: float) -> float:
        return hinge_z - (y - hinge_y) * math.tan(math.radians(rake))

    for sx in (-1.0, 1.0):
        _box(p, (sx * 0.245, (hinge_y + size[1]) * 0.5, back_z((hinge_y + size[1]) * 0.5)),
             (0.03, size[1] - hinge_y, 0.026), frame_uv, palette.shade(RESIN_TINT, 0.97),
             rotation=(-rake, 0.0, 0.0))
    for y in (0.50, 0.615, 0.73):
        _box(p, (0.0, y, back_z(y)), (0.40, 0.05, 0.022), slat_uv,
             palette.shade(RESIN_TINT, 1.01), rotation=(-rake, 0.0, 0.0))
    _box(p, (0.0, 0.822, back_z(0.822)), (0.50, 0.055, 0.03), slat_uv,
         palette.shade(RESIN_TINT, 1.02), rotation=(-rake, 0.0, 0.0))
    p.add_note("42 cm seat, raked slatted back, four tapered legs; matches the table")


# ------------------------------------------------------------------- ladder


def build_pool_ladder(p: PropBuilder) -> None:
    """Chrome pool ladder: two 2.2 m rails curving out over the deck edge at
    the top, five rungs at 0.3 m.  It stands on the basin floor, so the top
    0.7 m rises above the deck when the level places it at y = -1.5."""
    size = p.size  # [0.55, 2.2, 0.45]
    tex = p.set_texture(64, seed=233)
    tex.auto("tube", "rung")

    _paint_metal(tex, "tube", CHROME, 401)
    _paint_metal(tex, "rung", palette.shade(CHROME, 0.96), 407)

    tube_uv = tex.uv("tube")
    rung_uv = tex.uv("rung")
    radius = 0.025
    rail_x = 0.25
    # Vertical in the basin, then a gentle outward curve: the last 0.7 m of
    # the rail bends towards +Z, the deck side.  The footprint balances the
    # vertical stock behind against the curl in front, so the mesh is centred
    # inside the catalogue's 0.45 m depth.
    rail_z = -0.20
    for sx in (-1.0, 1.0):
        points = (
            (sx * rail_x, 0.0, rail_z),
            (sx * rail_x, 1.50, rail_z),
            (sx * rail_x, 1.88, rail_z + 0.06),
            (sx * rail_x, 2.07, rail_z + 0.20),
            (sx * rail_x, 2.19, rail_z + 0.40),
        )
        p.tube_path(points, radii=radius, segments=8, uv=tube_uv, color=CHROME_TINT,
                    cap_start=False, cap_end=True)
    for index in range(5):
        y = 0.25 + index * 0.30
        p.cylinder(
            (-rail_x, y, rail_z), 0.017, 2.0 * rail_x, axis="x", segments=6,
            side_uv=rung_uv, cap_uv=rung_uv, color=palette.shade(CHROME_TINT, 0.98),
            bottom=False,
        )
    p.add_note("curved rail sweeps +Z over the deck; five rungs at 0.30 m")


# ----------------------------------------------------------------- curtains


def _curtain_common(tex) -> tuple:
    """Paint the shared curtain sheet and return its UV regions."""
    _paint_cloth(tex, "cloth", CLOTH, 501)
    _paint_cloth(tex, "post", palette.shade(CLOTH, 0.92), 509)
    _paint_metal(tex, "rail", CHROME, 517)
    return tex.uv("cloth"), tex.uv("post"), tex.uv("rail")


def build_pool_curtain_straight(p: PropBuilder) -> None:
    """Freestanding privacy curtain, full module: two 25 mm posts, a top rail
    and a five-fold hanging panel (the folds are shallow planes, not cloth)."""
    size = p.size  # [1.2, 2.6, 0.22]
    tex = p.set_texture(128, seed=241)
    tex.auto("cloth", "post", "rail")
    cloth_uv, post_uv, rail_uv = _curtain_common(tex)

    post_x = size[0] * 0.5 - 0.0125      # 0.5875: the post owns the 1.2 m box
    top = 2.45
    bottom = 0.12
    for sx in (-1.0, 1.0):
        _curtain_post(p, sx * post_x, 0.0, post_uv, CLOTH_TINT)
    _curtain_rail(p, (-post_x, 2.55, 0.0), "x", 2.0 * post_x, rail_uv, CHROME_TINT)

    xs = [-0.55, -0.33, -0.11, 0.11, 0.33, 0.55]
    zs = [-0.11, 0.11, -0.11, 0.11, -0.11, 0.11]
    _hanging_panel(p, xs, zs, top, bottom, cloth_uv, CLOTH_TINT)
    p.add_note("five-fold panel between two 25 mm posts; folds are face shading")


def build_pool_curtain_end(p: PropBuilder) -> None:
    """Half-width end module that closes a curtain run: one post at its +X
    edge, a half rail and a three-fold panel hanging to the free end."""
    size = p.size  # [0.6, 2.6, 0.22]
    tex = p.set_texture(128, seed=251)
    tex.auto("cloth", "post", "rail")
    cloth_uv, post_uv, rail_uv = _curtain_common(tex)

    post_x = size[0] * 0.5 - 0.0125      # 0.2875
    _curtain_post(p, post_x, 0.0, post_uv, CLOTH_TINT)
    _curtain_rail(p, (-post_x, 2.55, 0.0), "x", 2.0 * post_x, rail_uv, CHROME_TINT)

    xs = [-0.30, -0.1167, 0.0667, 0.25]
    zs = [-0.11, 0.11, -0.11, 0.11]
    _hanging_panel(p, xs, zs, 2.45, 0.12, cloth_uv, CLOTH_TINT)
    p.add_note("one post at +X; panel hangs to the open -X end")


def build_pool_curtain_corner(p: PropBuilder) -> None:
    """L module turning a run 90 degrees: a shared corner post at (-0.2875,
    -0.2875) and two half panels, one per leg, folded inwards."""
    size = p.size  # [0.6, 2.6, 0.6]
    tex = p.set_texture(128, seed=257)
    tex.auto("cloth", "post", "rail")
    cloth_uv, post_uv, rail_uv = _curtain_common(tex)

    corner = -0.2875
    _curtain_post(p, corner, corner, post_uv, CLOTH_TINT)
    _curtain_rail(p, (corner, 2.55, corner), "x", 0.5875, rail_uv, CHROME_TINT)
    _curtain_rail(p, (corner, 2.55, corner), "z", 0.5875, rail_uv, CHROME_TINT)

    # Leg along +X: the panel folds between the post plane and 0.22 m inwards.
    xs = [-0.25, -0.0667, 0.1167, 0.30]
    zs = [corner, corner + 0.22, corner, corner + 0.22]
    _hanging_panel(p, xs, zs, 2.45, 0.12, cloth_uv, CLOTH_TINT)
    # Leg along +Z: mirrored about the corner.
    zs2 = [-0.25, -0.0667, 0.1167, 0.30]
    xs2 = [corner, corner + 0.22, corner, corner + 0.22]
    _hanging_panel(p, xs2, zs2, 2.45, 0.12, cloth_uv, CLOTH_TINT)
    p.add_note("shared corner post; two half panels folded inwards")


# --------------------------------------------------------------- guardrails


def _guardrail_common(tex):
    _paint_metal(tex, "post", SILVER, 601)
    _paint_metal(tex, "rail", palette.shade(SILVER, 1.02), 607)
    _paint_metal(tex, "plate", palette.shade(SILVER, 0.90), 613, warm=True)
    return tex.uv("post"), tex.uv("rail"), tex.uv("plate")


def build_pool_guardrail_straight(p: PropBuilder) -> None:
    """Silver guardrail section: a 2.0 m bay, three 40 mm posts and two
    horizontal rails at 1.0 m and 0.55 m, on bolted base plates."""
    size = p.size  # [2.0, 1.05, 0.08]
    tex = p.set_texture(64, seed=261)
    tex.auto("post", "rail", "plate")
    post_uv, rail_uv, plate_uv = _guardrail_common(tex)

    for x in (-0.98, 0.0, 0.98):
        _guardrail_post(p, x, 0.0, post_uv, SILVER_TINT)
    _guardrail_rail(p, (-1.0, 0.0), "x", 2.0, 0.98, 0.021, rail_uv, palette.shade(SILVER_TINT, 1.02))
    _guardrail_rail(p, (-1.0, 0.0), "x", 2.0, 0.53, 0.017, rail_uv, SILVER_TINT)
    for x in (-0.955, 0.0, 0.955):
        _guardrail_plate(p, x, 0.0, plate_uv, palette.shade(SILVER_TINT, 0.94))
    p.add_note("two rails on three posts plus base plates; joins post-to-post")


def build_pool_guardrail_end(p: PropBuilder) -> None:
    """Short guardrail return that terminates a run."""
    size = p.size  # [0.6, 1.05, 0.08]
    tex = p.set_texture(64, seed=263)
    tex.auto("post", "rail", "plate")
    post_uv, rail_uv, plate_uv = _guardrail_common(tex)

    for x in (-0.28, 0.28):
        _guardrail_post(p, x, 0.0, post_uv, SILVER_TINT)
    _guardrail_rail(p, (-0.3, 0.0), "x", 0.6, 0.98, 0.021, rail_uv, palette.shade(SILVER_TINT, 1.02))
    _guardrail_rail(p, (-0.3, 0.0), "x", 0.6, 0.53, 0.017, rail_uv, SILVER_TINT)
    for x in (-0.255, 0.255):
        _guardrail_plate(p, x, 0.0, plate_uv, palette.shade(SILVER_TINT, 0.94))
    p.add_note("short return: two posts, two rails, base plates")


def build_pool_guardrail_corner(p: PropBuilder) -> None:
    """L guardrail module: legs along +X and +Z from a shared corner post."""
    size = p.size  # [0.6, 1.05, 0.6]
    tex = p.set_texture(64, seed=267)
    tex.auto("post", "rail", "plate")
    post_uv, rail_uv, plate_uv = _guardrail_common(tex)

    for x, z in ((-0.28, -0.28), (0.28, -0.28), (-0.28, 0.28)):
        _guardrail_post(p, x, z, post_uv, SILVER_TINT)
    for axis in ("x", "z"):
        _guardrail_rail(p, (-0.28, -0.28), axis, 0.58, 0.98, 0.021, rail_uv,
                        palette.shade(SILVER_TINT, 1.02))
        _guardrail_rail(p, (-0.28, -0.28), axis, 0.58, 0.53, 0.017, rail_uv, SILVER_TINT)
    for x, z in ((-0.255, -0.255), (0.255, -0.255), (-0.255, 0.255)):
        _guardrail_plate(p, x, z, plate_uv, palette.shade(SILVER_TINT, 0.94))
    p.add_note("shared corner post; one rail pair per leg; base plates")


PROPS = {
    "core:pool_table": build_pool_table,
    "core:pool_chair": build_pool_chair,
    "core:pool_ladder": build_pool_ladder,
    "core:pool_curtain_straight": build_pool_curtain_straight,
    "core:pool_curtain_end": build_pool_curtain_end,
    "core:pool_curtain_corner": build_pool_curtain_corner,
    "core:pool_guardrail_straight": build_pool_guardrail_straight,
    "core:pool_guardrail_end": build_pool_guardrail_end,
    "core:pool_guardrail_corner": build_pool_guardrail_corner,
}
