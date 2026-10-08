"""Winter concept masonry and timber construction; only committed PNG loading."""
from pathlib import Path

from parts.refreshed import load_atlas_from, solid_box
from parts.outdoor_remade import rings, limb, fit
from parts.winter import _strip, SNOW

ROOT = Path(__file__).resolve().parents[3] / 'assets/environment/winter/props/models'
WHITE = (255, 255, 255)
SIZES = {
    'stone_wall_snow': (2.4, 1.10, .5),
    'masonry_pier_snow': (.65, 1.25, .65),
    'timber_lantern': (.65, 3.2, .6),
    'entrance_frame': (1.74, 2.42, .34),
    'door_hood_snow': (1.94, .55, .9),
    'window_frame': (1.26, 1.16, .21),
    'ice_fragment': (1.1, .075, .8),
}


def atlas(p):
    p.ao_strength = 0
    t = load_atlas_from(p, ROOT / 'village_materials.png', ('stone', 'wood', 'metal', 'glass'))
    p.begin_material(p.material('village_stock'))
    return t


def box(p, center, size, uv, color=WHITE):
    solid_box(p, center, size, uv=uv, color=color, shade=False)


def snow(p):
    p.begin_material(p.material('snow', use_texture=False))


def wall(p):
    t = atlas(p)
    for row in range(4):
        widths = (.68, .92, .776) if row % 2 else (.88, .67, .826)
        x = -1.2
        for col, width in enumerate(widths):
            box(p, (x+width/2, .12+row*.23, 0), (width-.008, .222, .40),
                t.uv('stone', inset=2), (235+col*7, 237+col*6, 242+col*4))
            x += width+.008
    box(p, (0, .955, 0), (2.4, .06, .45), t.uv('stone', inset=2))
    snow(p)
    _strip(p, 2.4, .12, .5, y=.98)
    fit(p)
    p.add_note('four real masonry courses, offset joints, projecting coping and continuous thick exposed snow cap')


def pier(p):
    t = atlas(p)
    box(p, (0, .045, 0), (.62, .09, .62), t.uv('stone', inset=2))
    for row in range(5):
        for side in (-1, 1):
            box(p, (side*.126, .197+row*.185, 0), (.244, .177, .5), t.uv('stone', inset=2),
                (235+row*3, 240+row*2, 248))
    box(p, (0, 1.095, 0), (.64, .13, .64), t.uv('stone', inset=2))
    snow(p)
    _strip(p, .65, .10, .65, y=1.151)
    fit(p)
    p.add_note('five staggered mineral courses and projecting foot/coping; snow on exposed top only')


def lantern(p):
    t = atlas(p)
    wood, metal = t.uv('wood', inset=2), t.uv('metal', inset=2)
    box(p, (0, .045, 0), (.33, .09, .33), t.uv('stone', inset=2))
    box(p, (0, 1.33, 0), (.155, 2.6, .155), wood)
    box(p, (0, 2.65, .11), (.12, .12, .46), wood)
    limb(p, (0, 2.20, .04), (0, 2.62, .25), .035, .035, wood)
    p.begin_material(p.material('lantern_metal'))
    box(p, (0, 2.67, .16), (.42, .045, .40), metal)
    for x, z in ((-1,-1), (1,-1), (1,1), (-1,1)):
        limb(p, (x*.145, 2.69, .16+z*.135), (x*.21, 2.99, .16+z*.20), .012, .012, metal)
    rings(p, [[(x*.30, 2.995, .16+z*.285) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))],
              [(x*.035, 3.12, .16+z*.035) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))]], metal)
    p.begin_material(p.material('lantern_amber', emissive=(1,.78,.42), strength=1.0))
    rings(p, [[(x*.14, 2.69, .16+z*.13) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))],
              [(x*.195, 2.985, .16+z*.185) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))]], t.uv('glass', inset=2))
    snow(p)
    # Snow sits on the four pitched hood faces, roots sunk into the metal.
    rings(p, [[(x*.30, 2.991, .16+z*.285) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))],
              [(x*.325, 3.045, .16+z*.30) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))],
              [(x*.04, 3.20, .16+z*.04) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))]], (0,0,1,1), SNOW)
    fit(p)
    p.add_note('straight timber post, knee bracket, tapered four-pane lantern and snow-loaded pyramidal hood; emitter below glazing')


def entrance(p):
    t = atlas(p)
    for side in (-1,1):
        for row in range(7):
            box(p, (side*.74, .166+row*.315, 0), (.26, .307, .30),
                t.uv('stone', inset=2), (246-row*2, 248-row*2, 255-row))
        box(p, (side*.74, .055, 0), (.30,.11,.34), t.uv('stone', inset=2))
    for col in (-1,0,1):
        box(p, (col*.55, 2.29, 0), (.542,.26,.32), t.uv('stone', inset=2))
    # Separate recessed timber liner; leaves the real 1.14 x 2.15 doorway clear.
    for side in (-1,1):
        box(p, (side*.596, 1.066, -.05), (.026,2.132,.13), t.uv('wood', inset=2))
    fit(p)
    p.add_note('closed coursed masonry jambs, hewn lintel and recessed timber liner; open centre uses the existing operable door')


def hood(p):
    t = atlas(p)
    wood = t.uv('wood', inset=2)
    for x in (-.72,.72):
        limb(p, (x,.04,-.34), (x,.32,.28), .045, .045, wood)
        box(p, (x,.29,0), (.11,.10,.78), wood)
    box(p, (0,.35,.32), (1.86,.13,.12), wood)
    levels = [[(x*.93, yy+(.08 if z<0 else 0), z*.39) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))]
              for yy in (.34,.405)]
    rings(p, levels, wood)
    snow(p)
    # Only the projecting nose lies beyond the main roof's shelter. The
    # inner roof and wall brackets remain bare timber.
    _strip(p, 1.94, .125, .30, z=.30, y=.445-(.08/.78)*.30, slope=.08/.78)
    fit(p)
    p.add_note('projecting closed timber hood, two diagonal supports and thick snow roof; high side faces the wall (-Z)')


def window(p):
    t = atlas(p)
    wood = t.uv('wood', inset=2)
    for x in (-.567,.567):
        box(p, (x,.58,-.015), (.08,1.10,.12), wood)
    for y in (.065,1.095):
        box(p, (0,y,0), (1.08,.08,.14), wood)
    box(p, (0,.026,.025), (1.26,.052,.21), wood)
    box(p, (0,.58,.018), (.038,.956,.075), wood)
    box(p, (0,.58,.021), (1.05,.038,.08), wood)
    fit(p)
    p.add_note('real deep timber sill, recessed jambs and cross mullions around the existing glazed opening')


def ice(p):
    p.ao_strength = 0
    load_atlas_from(p, ROOT / 'ice_surface.png', ())
    p.begin_material(p.material('ice'))
    points = [(-.55,0,-.18),(-.22,0,-.40),(.39,0,-.32),(.55,0,.12),(.12,0,.40),(-.48,0,.29)]
    rings(p, [points, [(x,.075,z) for x,_,z in points]], (0,0,1,1), (205,230,255))
    fit(p)
    p.add_note('closed thin angular frozen fragment; ordinary opaque static prop, no unsupported GLB blend')


PROPS = {'winter:'+name: build for name,build in {
    'stone_wall_snow':wall, 'masonry_pier_snow':pier, 'timber_lantern':lantern,
    'entrance_frame':entrance, 'door_hood_snow':hood, 'window_frame':window,
    'ice_fragment':ice,
}.items()}
