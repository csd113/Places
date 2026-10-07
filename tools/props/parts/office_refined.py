"""Office-only refinements, preserving the five existing ids and footprints.

Artwork is loaded from committed native PNGs. The shared primitive toolkit,
lighting and material catalog are deliberately unchanged.
"""
from pathlib import Path

import glb
from parts.refreshed import load_atlas_from, padded_box, solid_box, outward_lathe

ROOT = Path(__file__).resolve().parents[3] / "assets/environment/office/props/models"
WHITE = (255, 255, 255)


def atlas(p, names):
    return load_atlas_from(p, ROOT / (p.id.split(":")[1] + ".png"), names)


def box(p, center, size, uv, color=WHITE):
    solid_box(p, center, size, uv=uv, color=color)


def pull(p, x, y, z, uv, width):
    """Three closed pieces: mounted bridges, with visible space behind a pull."""
    for dx in (-width / 2 + .008, width / 2 - .008):
        box(p, (x + dx, y, z - .008), (.016, .018, .024), uv)
    box(p, (x, y, z + .005), (width, .018, .010), uv)


def desk(p):
    t = atlas(p, ("top", "body", "drawer", "metal"))
    top, body, drawer, metal = [t.uv(name, inset=2) for name in ("top", "body", "drawer", "metal")]
    # A modest edge bevel spends triangles only on the prominent desktop.
    padded_box(p, (0, .729, 0), (1.6, .042, .7), top, bevel=.005)
    box(p, (0, .410, -.294), (1.47, .48, .030), body)
    for x in (-.565, .565):
        box(p, (x, .36, -.018), (.36, .64, .60), body)
        box(p, (x, .020, -.018), (.32, .040, .55), metal, (160, 160, 160))
        for y, height in ((.535, .225), (.277, .257)):
            # Faces stand clear of the carcass, with real drawer gaps.
            box(p, (x, y, .289), (.326, height, .018), drawer)
            pull(p, x, y + .060, .323, metal, .11)
    # The central shallow pencil drawer leaves knee clearance below.
    box(p, (0, .660, .246), (.70, .078, .10), body)
    box(p, (0, .660, .303), (.66, .056, .014), drawer)
    pull(p, 0, .665, .332, metal, .15)
    p.add_note("oak laminate top, twin metal pedestals, five separated drawer faces and bridge pulls")


def cabinet(p):
    t = atlas(p, ("body", "door", "edge", "metal"))
    body, face, edge, metal = [t.uv(name, inset=2) for name in ("body", "door", "edge", "metal")]
    box(p, (0, .030, -.01), (.83, .060, .40), edge)
    # Stop the carcass behind the drawer faces; burying the faces in a solid
    # box hides their reveals and label holders in the real renderer.
    box(p, (0, .431, -.029), (.868, .738, .376), body)
    padded_box(p, (0, .825, 0), (.9, .05, .45), body, bevel=.005)
    for y in (.245, .611):
        box(p, (0, y, .170), (.816, .346, .022), face)
        pull(p, 0, y + .040, .215, metal, .16)
        # Label holders are recessed rectangles, large enough to read at Low.
        box(p, (0, y + .119, .183), (.118, .047, .006), metal)
        box(p, (0, y + .119, .187), (.096, .030, .003), body, (235, 229, 206))
    p.add_note("two broad filing drawers, label holders, metal bridge handles and recessed plinth; original footprint")


def chair(p):
    # The shipped GLB includes subsequent geometry repairs not represented by
    # the primitive painter. Treat it as the authored geometry source: a
    # texture-only export must retain those repairs, UVs and vertex colours.
    source = glb.read_glb((ROOT / "chair.glb").read_bytes())
    atlas(p, ("shell", "pad", "metal", "dark"))
    p.mesh.positions = list(source.positions)
    p.mesh.uvs = list(source.uvs)
    p.mesh.indices = list(source.indices)
    p.mesh.colors = [tuple(round(value * 255) for value in color) for color in source.colors]
    p.add_note("retained existing 704-triangle chair construction; authored woven upholstery atlas")


def water_cooler(p):
    t = atlas(p, ())
    # Preserve the previous two-column / three-row UV frame, including its
    # unused final cell (integer thirds match Texture.auto).
    t.auto("front", "side", "bottle", "cap", "top", cols=2, rows=3)
    front, side, bottle, cap, top = [t.uv(name, inset=2) for name in ("front", "side", "bottle", "cap", "top")]
    box(p, (0, .024, 0), (.31, .048, .30), side, (155, 155, 150))
    # A genuinely open tap recess: no full cabinet box behind the cavity.
    box(p, (0, .228, -.009), (.35, .36, .332), front)
    box(p, (0, .628, -.009), (.35, .064, .336), top)
    for x in (-.150, .150):
        box(p, (x, .500, -.009), (.05, .20, .332), side)
    box(p, (0, .500, -.099), (.25, .20, .15), side, (160, 163, 155))
    box(p, (0, .414, .056), (.25, .012, .204), top, (160, 163, 155))
    for x, color in ((-.063, (188, 97, 86)), (.063, (86, 120, 168))):
        box(p, (x, .540, -.005), (.029, .036, .048), top)
        box(p, (x, .540, .023), (.024, .012, .019), top, color)
        box(p, (x, .515, .031), (.019, .036, .021), top)
    # Separate drip-tray slats and a lower access-panel reveal.
    for x in (-.090, -.060, -.030, 0, .030, .060, .090):
        box(p, (x, .422, .070), (.013, .003, .11), side, (100, 107, 103))
    box(p, (0, .201, .160), (.273, .28, .008), front)
    outward_lathe(p, (0, 0, -.01), [(.66, .070), (.69, .070), (.72, .054)],
                  segments=8, uv=top, color=WHITE, cap_start=True, cap_end=True)
    # Bottle ribs are part of one closed shell, avoiding duplicated cylinders.
    profile = [(.70, .050), (.735, .050), (.77, .122), (.80, .139),
               (.827, .139), (.833, .145), (.847, .145), (.853, .139),
               (.94, .139), (.946, .145), (.960, .145), (.966, .139),
               (1.077, .139), (1.087, .145), (1.098, .137), (1.10, .124)]
    outward_lathe(p, (0, 0, -.01), profile, segments=8, uv=bottle, color=WHITE,
                  cap_start=True, cap_end=True)
    p.add_note("open tap recess, two coloured controls, drip tray, continuous eight-sided ribbed blue bottle")


def vending_machine(p):
    t = atlas(p, ("front", "panel", "side", "trim"))
    front, panel, side, trim = [t.uv(name, inset=2) for name in ("front", "panel", "side", "trim")]
    # A plain closed carcass; fitted artwork is applied once, never repeated
    # on the proud rails or hardware.
    box(p, (0, .030, -.015), (.90, .060, .73), trim)
    box(p, (0, .958, -.022), (1, 1.884, .756), side)
    for x in (-.478, .143, .478):
        box(p, (x, .980, .378), (.038, 1.814, .044), trim)
    for y in (.094, 1.867):
        box(p, (0, y, .378), (.918, .036, .044), trim)
    box(p, (-.164, .974, .358), (.59, 1.735, .014),
        {"+z": front, "-z": side, "+x": trim, "-x": trim, "+y": trim, "-y": trim})
    box(p, (.312, .974, .366), (.291, 1.735, .014),
        {"+z": panel, "-z": side, "+x": trim, "-x": trim, "+y": trim, "-y": trim})
    # Raised buttons exactly follow their fitted atlas positions.
    for u in (.32, .68):
        for v in (.26, .38, .50, .62, .74):
            x = .1665 + .291 * u
            y = 1.8415 - 1.735 * v
            u0, v0, u1, v1 = panel
            button_uv = (u0+(u1-u0)*(u-.05), v0+(v1-v0)*(v-.024),
                         u0+(u1-u0)*(u+.05), v0+(v1-v0)*(v+.024))
            box(p, (x, y, .383), (.027, .039, .017), button_uv)
    # The delivery flap sits on its painted opening without hiding the scene.
    box(p, (-.249, .376, .381), (.267, .035, .016), trim)
    box(p, (.317, .334, .382), (.101, .035, .018), trim)
    p.add_note("mountain drinks fascia, recessed face, raised selection buttons, coin bezel, delivery lip and inset plinth")
