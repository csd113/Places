"""Closed low-poly cable spans with separate dark and amber material slots."""
from pathlib import Path

from parts.refreshed import load_atlas_from, orient_outward, solid_cylinder, solid_box
from string_lights import SPANS, CABLE_RADIUS, BULB_HEIGHT, DROP, cable_height, bulbs, attachment_height

SOURCE = Path(__file__).resolve().parents[3] / 'assets/environment/winter/props/models/string_lights.png'


def build(p, variant):
    tex = load_atlas_from(p, SOURCE, ('glass', 'cable', 'clip'))
    body = p.material('dark_cable')
    glass = p.material('warm_bulb', emissive=(1, .56, .16), strength=1.0)
    length, _, count = SPANS[variant]
    p.begin_material(body)
    points = [(length*(step/(count*2)-.5),
               cable_height(variant, length*(step/(count*2)-.5)), 0)
              for step in range(count*2+1)]
    p.tube_path(points, radii=CABLE_RADIUS, segments=4, uv=tex.uv('cable'),
                color=(45, 40, 34), cap_start=True, cap_end=True)
    for x in (-length/2, length/2):
        solid_box(p, (x, attachment_height(variant), 0), (.05, .05, .04),
                  uv=tex.uv('clip'), color=(50, 45, 36), shade=False)
    for x, top in bulbs(variant):
        solid_cylinder(p, (x, top-.015, 0), .022, .045, segments=6,
                       uv=tex.uv('clip'), color=(55, 46, 31), shades=False)
        solid_cylinder(p, (x, top+.03, 0), CABLE_RADIUS, DROP-.03, segments=4,
                       uv=tex.uv('cable'), color=(45, 40, 34), shades=False)
    p.begin_material(glass)
    for x, top in bulbs(variant):
        center = (x, top-BULB_HEIGHT/2, 0)
        start = len(p.mesh.indices)
        p.lathe(center, [(-.07, .012), (-.035, .043), (.025, .045), (.07, .016)], segments=6,
                uv=tex.uv('glass'), color=(255, 216, 135), shades=False)
        orient_outward(p.mesh, start, center)
    p.add_note('attachment centres at +/- span/2; root is the lowest bulb tip')
    p.add_note('amber emission 1.0; actual static sources authored below each bulb')


PROPS = {'winter:string_lights_'+variant: (lambda p, v=variant: build(p, v)) for variant in SPANS}
