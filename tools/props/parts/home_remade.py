"""Home-specific closed construction, loading committed fitted PNG atlases.

Shared core furniture/appliances retain their original models and imagery.
"""
from pathlib import Path

from parts.domestic_remade import loft, section
from parts.refreshed import load_atlas_from, padded_box, solid_box, solid_cylinder, outward_lathe

ROOT = Path(__file__).resolve().parents[3] / "assets/environment/home/props/models"
WHITE = (255, 255, 255)


def atlas(p, names=("main", "secondary", "detail", "dark")):
    return load_atlas_from(p, ROOT / (p.id.split(":")[1] + ".png"), names)


def box(p, center, size, uv):
    solid_box(p, center, size, uv=uv, color=WHITE)


def shaker(p, x, bottom, width, height, front, paint, recess):
    # The four real rails stand 12mm in front of the recessed centre panel.
    rail = .030
    box(p, (x, bottom + height / 2, front - .012), (width, height, .018), recess)
    for xx in (x - width / 2 + rail / 2, x + width / 2 - rail / 2):
        box(p, (xx, bottom + height / 2, front), (rail, height, .018), paint)
    for yy in (bottom + rail / 2, bottom + height - rail / 2):
        box(p, (x, yy, front), (width - 2 * rail - .001, rail, .018), paint)


def pull(p, x, y, z, metal, horizontal=False):
    for s in (-1, 1):
        xx, yy = (x + s * .034, y) if horizontal else (x, y + s * .034)
        solid_cylinder(p, (xx, yy, z), .003, .018, segments=6, axis="z", uv=metal, color=WHITE)
    box(p, (x, y, z + .021), (.082, .008, .008) if horizontal else (.008, .082, .008), metal)


def cabinet(p, upper=False, sink=False):
    t = atlas(p, ("body", "door", "counter", "metal"))
    paint, recess = t.uv("body", inset=2), t.uv("door", inset=2)
    metal, counter = t.uv("metal", inset=2), t.uv("counter", inset=2)
    height, depth = (.72, .33) if upper else (.86, .59)
    bottom = 0 if upper else .10
    # Recessed kick, continuous carcass and a solid side thickness.
    box(p, (0, (height + bottom) / 2, -.029), (.60, height - bottom, depth - .062), paint)
    if not upper:
        box(p, (0, .050, -.044), (.55, .100, .48), metal)
    front = depth / 2 - .035
    door_bottom, door_height = (.020, .680) if upper else (.11, .58)
    for x in (-.147, .147):
        shaker(p, x, door_bottom, .278, door_height, front, paint, recess)
        pull(p, -.041 if x < 0 else .041, door_bottom + door_height - .085, front + .010, metal)
    if not upper:
        shaker(p, 0, .705, .566, .139, front, paint, recess)
        pull(p, 0, .773, front + .010, metal, horizontal=True)
        if not sink:
            # 40mm laminate edge: unbroken flat deck, fitted quiet mineral art.
            box(p, (0, .880, 0), (.60, .040, .60), counter)
        else:
            # Real open basin, with a closed folded-steel shell and drain.
            for x in (-.26, .26):
                box(p, (x, .88, 0), (.08, .04, .60), counter)
            for z in (-.22, .22):
                box(p, (0, .88, z), (.438, .04, .16), counter)
            box(p, (0, .748, 0), (.44, .018, .28), counter)
            for x in (-.211, .211):
                box(p, (x, .813, 0), (.018, .110, .28), counter)
            for z in (-.131, .131):
                box(p, (0, .813, z), (.402, .110, .018), counter)
            solid_cylinder(p, (0, .758, 0), .022, .003, segments=10, uv=metal, color=WHITE)
            solid_cylinder(p, (0, .901, -.22), .019, .012, segments=10, uv=counter, color=WHITE)
            solid_cylinder(p, (0, .91, -.22), .009, .178, segments=8, uv=metal, color=WHITE)
            solid_cylinder(p, (0, 1.08, -.215), .009, .118, segments=8, axis="z", uv=metal, color=WHITE)
            solid_cylinder(p, (0, 1.05, -.097), .009, .035, segments=8, uv=metal, color=WHITE)
    p.add_note("closed painted Shaker construction; real recessed centres, rails, drawer, standoff pulls and toe recess")


def cabinet_base(p): cabinet(p)
def cabinet_wall(p): cabinet(p, upper=True)
def sink(p): cabinet(p, sink=True)


def sofa(p):
    t = atlas(p, ("body", "seat", "back", "wood"))
    w, h, d = p.size
    for x in (-w / 2 + .13, w / 2 - .13):
        for z in (-d / 2 + .13, d / 2 - .13):
            solid_cylinder(p, (x, 0, z), .043, .15, segments=6, taper=.8, uv=t.uv("wood"), color=WHITE)
    padded_box(p, (0, .245, 0), (w - .04, .25, d - .04), t.uv("body", inset=2), bevel=.022)
    padded_box(p, (0, .635, -d / 2 + .11), (w - .04, .53, .22), t.uv("body", inset=2), bevel=.028)
    for x in (-w / 2 + .11, w / 2 - .11):
        padded_box(p, (x, .475, .035), (.22, .42, d - .02), t.uv("body", inset=2), bevel=.023)
    count = 3 if w > 1 else 1
    cw = (w - .47) / count
    for i in range(count):
        x = (i - (count - 1) / 2) * (cw + .007)
        padded_box(p, (x, .407, .075), (cw, .147, d - .22), t.uv("seat", inset=2), bevel=.024)
        padded_box(p, (x, .668, -.187), (cw, .386, .16), t.uv("back", inset=2), bevel=.026, rotation=(-9, 0, 0))
    p.add_note("cream woven upholstery, distinct cushions, square arms, tapered timber feet")


def table(p, coffee=False):
    t = atlas(p)
    p.begin_material(p.material("varnished_timber", roughness=.72))
    w, h, d = p.size
    top = .055 if coffee else .040
    padded_box(p, (0, h - top / 2, 0), (w, top, d), t.uv("main", inset=2), bevel=.006)
    for x in (-w / 2 + .08, w / 2 - .08):
        for z in (-d / 2 + .07, d / 2 - .07):
            box(p, (x, (h - top) / 2, z), (.065, h - top, .065), t.uv("secondary", inset=2))
    for z in (-d / 2 + .058, d / 2 - .058):
        box(p, (0, h - top - .049, z), (w - .16, .085, .024), t.uv("secondary", inset=2))
    for x in (-w / 2 + .058, w / 2 - .058):
        box(p, (x, h - top - .049, 0), (.024, .085, d - .16), t.uv("secondary", inset=2))
    if coffee:
        padded_box(p, (0, .119, 0), (w - .13, .028, d - .13), t.uv("detail", inset=2), bevel=.004)
    p.add_note("timber slab, 65mm floor-contact legs, jointed aprons" + (", lower shelf" if coffee else ""))


def coffee_table(p): table(p, True)
def dining_table(p): table(p)


def dining_chair(p):
    t = atlas(p)
    for x in (-.203, .203):
        box(p, (x, .451, -.213), (.05, .902, .05), t.uv("secondary", inset=2))
        box(p, (x, .212, .204), (.05, .424, .05), t.uv("secondary", inset=2))
    padded_box(p, (0, .448, 0), (.50, .045, .49), t.uv("main", inset=2), bevel=.006)
    for y in (.64, .817):
        padded_box(p, (0, y, -.213), (.40, .118, .045), t.uv("detail", inset=2), bevel=.006)
    for x in (-.203, .203):
        box(p, (x, .207, 0), (.025, .03, .395), t.uv("dark", inset=2))
    p.add_note("domestic wooden side chair: tall rear stiles, two broad back slats, seat, front legs and stretchers")


def dining_chair_refined(p):
    t = load_atlas_from(p, ROOT / "dining_chair.png", ("main", "secondary", "detail", "dark"))
    p.begin_material(p.material("varnished_timber", roughness=.72))

    def stock_ring(x, y, z, width, depth):
        return [(x + a * width / 2, y, z + b * depth / 2) for a, b in
                ((-1, -.65), (-.65, -1), (.65, -1), (1, -.65),
                 (1, .65), (.65, 1), (-.65, 1), (-1, .65))]

    for x in (-.203, .203):
        # The rear stock bends above the seat; the feet retain the floor contact.
        loft(p, [stock_ring(x, 0, -.178, .05, .05),
                 stock_ring(x, .43, -.178, .05, .05),
                 stock_ring(x, .902, -.218, .05, .05)], t.uv("secondary", inset=2))
        loft(p, [stock_ring(x - .015 if x < 0 else x + .015, 0, .222, .035, .035),
                 stock_ring(x, .424, .204, .05, .05)], t.uv("secondary", inset=2))
    padded_box(p, (0, .448, 0), (.50, .045, .49), t.uv("main", inset=2), bevel=.009)

    def slat_ring(x, y, height):
        return [(x, y + a * height / 2,
                 -.178 - (y + a * height / 2 - .43) * (.04 / .472) + b * .045 / 2)
                for a, b in ((-1, -.65), (-.65, -1), (.65, -1), (1, -.65),
                             (1, .65), (.65, 1), (-.65, 1), (-1, .65))]

    loft(p, [slat_ring(-.20, .64, .118), slat_ring(.20, .64, .118)],
         t.uv("detail", inset=2))
    # A shallow crown gives the broad upper slat a domestic silhouette.
    loft(p, [slat_ring(-.20, .817, .098), slat_ring(-.14, .827, .13),
             slat_ring(.14, .827, .13), slat_ring(.20, .817, .098)],
         t.uv("detail", inset=2))
    for x in (-.203, .203):
        box(p, (x, .207, 0), (.025, .03, .395), t.uv("dark", inset=2))
    p.add_note("domestic side chair: reclined chamfered rear stock, crowned back slat, eased seat and tapered front feet")


def tv_console(p):
    t = atlas(p)
    for x in (-.65, .65):
        for z in (-.14, .14): box(p, (x, .055, z), (.065, .11, .065), t.uv("secondary"))
    for y in (.128, .52):
        padded_box(p, (0, y, 0), (1.55, .06, .45), t.uv("main", inset=2), bevel=.005)
    for x in (-.75, .75, -.26, .26):
        box(p, (x, .322, -.005), (.05, .326, .42), t.uv("secondary", inset=2))
    box(p, (0, .322, -.212), (1.45, .328, .018), t.uv("dark", inset=2))
    box(p, (0, .288, .0), (.47, .024, .39), t.uv("main", inset=2))
    for x in (-.507, .507):
        shaker(p, x, .166, .42, .315, .200, t.uv("main", inset=2), t.uv("secondary", inset=2))
        pull(p, x + (.15 if x < 0 else -.15), .388, .213, t.uv("dark"))
    # Books and a small receiver occupy the open cubby, as in the reference.
    for i, height in enumerate((.087, .105, .073)):
        box(p, (-.105 + i * .085, .159 + height / 2, .02), (.065, height, .20), t.uv("dark"))
    box(p, (0, .339, .08), (.365, .072, .245), t.uv("detail"))
    p.add_note("low CRT console with thick top/sides, two Shaker cupboards, divided open receiver/book cubby and feet")


def bookshelf(p):
    t = atlas(p)
    for x in (-.475, .475): box(p, (x, .9, 0), (.05, 1.8, .35), t.uv("main", inset=2))
    box(p, (0, .9, -.163), (.897, 1.744, .024), t.uv("secondary", inset=2))
    for y in (.025, .45, .885, 1.32, 1.775):
        box(p, (0, y, 0), (.898, .05, .35), t.uv("main", inset=2))
    colors = [(142, 142, 117), (103, 122, 121), (187, 148, 114), (135, 110, 105)]
    for row in range(4):
        for i in range(6):
            height = .18 + ((row + i * 3) % 4) * .034
            solid_box(p, (-.35 + i * .10, .052 + row * .43 + height / 2, .010),
                      (.060 + (i % 2) * .022, height, .225), uv=t.uv("detail"), color=colors[(row + i) % 4])
    p.add_note("timber open shelving with real side stock, thin back, four shelves and individually readable book spines")


def rug(p):
    t = atlas(p)
    box(p, (0, .007, 0), (2.0, .014, 1.4), t.uv("secondary", inset=2))
    padded_box(p, (0, .015, 0), (1.975, .010, 1.375), t.uv("main", inset=2), bevel=.002)
    p.add_note("plain cream loop-pile rug with thin bound edge; floor-contact underside")


def floor_lamp(p):
    t = atlas(p)
    solid_cylinder(p, (0, 0, 0), .16, .037, segments=12, uv=t.uv("dark"), color=WHITE)
    solid_cylinder(p, (0, .033, 0), .015, 1.155, segments=8, uv=t.uv("secondary"), color=WHITE)
    p.begin_material(p.material("linen_shade", emissive=(1, .82, .48), strength=.32))
    outward_lathe(p, (0, 0, 0), [(1.173, .171), (1.18, .175), (1.487, .118),
                                (1.5, .112), (1.494, .102), (1.184, .162), (1.173, .171)],
                  segments=12, uv=t.uv("main", inset=2), color=WHITE, cap_start=False, cap_end=False)
    p.add_note("closed 12-sided warm linen lampshade, metal stem and floor-contact disc; illumination is map-owned")


def mug(p):
    t = atlas(p)
    outward_lathe(p, (-.010, 0, 0), [(0, .031), (.006, .038), (.084, .039),
                                    (.09, .037), (.09, .032), (.015, .031), (.011, .024)],
                  segments=16, uv=t.uv("main", inset=2), color=WHITE, cap_start=True, cap_end=True)
    # Three closed pieces form a genuine handle aperture, with bevelled joins.
    for y in (.026, .073):
        padded_box(p, (.046, y, 0), (.042, .013, .019), t.uv("secondary", inset=2), bevel=.003)
    padded_box(p, (.062, .0495, 0), (.012, .060, .019), t.uv("secondary", inset=2), bevel=.003)
    solid_cylinder(p, (-.010, .067, 0), .030, .002, segments=16, uv=t.uv("detail"), color=WHITE)


def book_stack(p):
    t = atlas(p)
    for y, name in ((.014, "main"), (.045, "secondary")):
        padded_box(p, (0, y, 0), (.25, .024, .18), t.uv(name, inset=2), bevel=.002)
        solid_box(p, (0, y, .001), (.242, .015, .173), uv=t.uv("dark", inset=2), color=WHITE)


def cushion(p):
    t = atlas(p)
    padded_box(p, (0, .16, 0), (.33, .32, .115), t.uv("main", inset=2), bevel=.043)
    p.add_note("checked cloth throw cushion; silhouette remains readable at 256px native")


def outlet(p):
    t = atlas(p)
    padded_box(p, (0, .06, 0), (.085, .12, .01), t.uv("main", inset=2), bevel=.003)
    for y in (.035, .085):
        padded_box(p, (0, y, .008), (.037, .031, .008), t.uv("secondary", inset=2), bevel=.006)
        for x in (-.008, .008): box(p, (x, y + .004, .0125), (.003, .009, .001), t.uv("detail"))
        box(p, (0, y - .007, .0125), (.005, .004, .001), t.uv("detail"))
    for y in (.010, .110):
        solid_cylinder(p, (0, y, .005), .0028, .001, segments=6, axis="z", uv=t.uv("dark"), color=WHITE)


def cabinet_strip(p):
    t = atlas(p)
    box(p, (0, .013, 0), (.55, .026, .07), t.uv("detail", inset=2))
    p.begin_material(p.material("warm_diffuser", emissive=(1, .83, .52), strength=.7))
    box(p, (0, .003, .005), (.51, .006, .054), t.uv("secondary", inset=2))


def landscape_frame(p):
    t = atlas(p)
    box(p, (0, .25, -.006), (.449, .499, .012), t.uv("dark", inset=2))
    for x in (-.21, .21): box(p, (x, .25, .004), (.03, .50, .028), t.uv("secondary", inset=2))
    for y in (.015, .485): box(p, (0, y, .004), (.389, .030, .028), t.uv("secondary", inset=2))
    box(p, (0, .25, .002), (.39, .44, .008), t.uv("detail", inset=2))
    box(p, (0, .25, .007), (.335, .374, .002), t.uv("main", inset=2))


def stove(p):
    t = atlas(p)
    paint, glass, metal, dark = [t.uv(n, inset=2) for n in ("main", "secondary", "detail", "dark")]
    box(p, (0, .443, 0), (.60, .886, .59), paint)
    box(p, (0, .897, 0), (.60, .018, .60), metal)
    for x in (-.146, .146):
        for z in (-.150, .155):
            solid_cylinder(p, (x, .906, z), .082, .006, segments=12, uv=dark, color=WHITE)
            solid_cylinder(p, (x, .912, z), .046, .004, segments=8, uv=glass, color=WHITE)
    box(p, (0, .429, .301), (.53, .51, .016), metal)
    padded_box(p, (0, .445, .312), (.455, .337, .012), glass, bevel=.010)
    pull(p, 0, .641, .316, dark, horizontal=True)
    box(p, (0, .79, .305), (.552, .107, .014), paint)
    for x in (-.22, -.11, .11, .22):
        solid_cylinder(p, (x, .792, .315), .024, .018, segments=10, axis="z", uv=dark, color=WHITE)
    box(p, (0, .79, .315), (.086, .034, .005), glass)
    for i in range(5): box(p, (0, .061 + i * .014, .297), (.50, .004, .008), dark)
    p.mesh.positions[:] = [(x, y, z - .021) for x, y, z in p.mesh.positions]
    p.add_note("cream enamel cooker, four cast burner rings, recessed oven lens, metal surround, knobs, handle and lower ventilation")


def kettle(p):
    t = atlas(p)
    outward_lathe(p, (0, 0, 0), [(0, .064), (.018, .079), (.047, .081),
                                (.147, .070), (.164, .059), (.174, .035)],
                  segments=12, uv=t.uv("main", inset=2), color=WHITE)
    solid_cylinder(p, (0, .173, 0), .036, .011, segments=12, uv=t.uv("detail"), color=WHITE)
    solid_cylinder(p, (0, .184, 0), .014, .017, segments=8, uv=t.uv("secondary"), color=WHITE)
    # Closed handle stock with a genuine opening; short inclined pour spout.
    for y in (.045, .130): box(p, (-.079, y, 0), (.044, .018, .018), t.uv("secondary"))
    box(p, (-.098, .0875, 0), (.015, .103, .018), t.uv("secondary"))
    loft(p, [section(-.010, .075, .072, .036, .044),
             section(.001, .103, .120, .026, .032),
             section(.012, .106, .132, .022, .026)], t.uv("detail", inset=2))


def toaster(p):
    t = atlas(p)
    padded_box(p, (0, .080, 0), (.25, .160, .16), t.uv("main", inset=2), bevel=.014)
    for z in (-.034, .034):
        box(p, (0, .160, z), (.183, .002, .017), t.uv("secondary", inset=2))
    solid_cylinder(p, (.115, .071, .080), .012, .008, segments=8, axis="z", uv=t.uv("dark"), color=WHITE)
    box(p, (0, .027, .080), (.155, .004, .003), t.uv("detail"))


def door_casing(p):
    t = atlas(p)
    # Three joined 70mm timber bars around a 1.40 x 2.10m clear opening.
    for x in (-.755, .755):
        box(p, (x, 1.0495, -.010), (.07, 2.099, .040), t.uv("main", inset=2))
        box(p, (x, 1.0495, .017), (.045, 2.099, .014), t.uv("secondary", inset=2))
    box(p, (0, 2.14, -.010), (1.58, .080, .040), t.uv("main", inset=2))
    box(p, (0, 2.14, .017), (1.579, .052, .014), t.uv("secondary", inset=2))
    p.add_note("real stepped painted casing profile around the existing opening; no leaf or gameplay behaviour")


PROPS = {
    "home:cabinet_base": cabinet_base, "home:cabinet_wall": cabinet_wall,
    "home:sofa": sofa, "home:armchair": sofa,
    "home:coffee_table": coffee_table, "home:dining_table": dining_table,
    "home:dining_chair": dining_chair, "home:tv_console": tv_console,
    "home:dining_chair_refined": dining_chair_refined,
    "home:bookshelf": bookshelf, "home:rug": rug,
    "home:floor_lamp": floor_lamp, "home:mug": mug,
    "home:book_stack": book_stack, "home:cushion": cushion,
    "home:outlet": outlet, "home:cabinet_strip": cabinet_strip,
    "home:landscape_frame": landscape_frame, "home:sink": sink, "home:stove": stove,
    "home:kettle": kettle, "home:toaster": toaster,
    "home:door_casing": door_casing,
}
