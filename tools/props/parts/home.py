"""Home registry and file-backed opal globe; models are authored offline."""
import math

from parts import domestic_remade, home_remade
from parts.refreshed import load_atlas_from, outward_lathe, solid_cylinder


def ball_light(p):
    t = load_atlas_from(p, home_remade.ROOT / "ball_light.png", ("orb", "cord", "rose"))
    p.begin_material(p.material("orb_glow", emissive=(1, .92, .74), strength=.8))
    radius = .19
    pole = .014
    span = math.sqrt(radius * radius - pole * pole)
    profile = [(-span, pole)] + [(radius * math.sin(math.radians(a)),
                                 radius * math.cos(math.radians(a)))
                                for a in (-60, -30, 0, 30, 60)] + [(span, pole)]
    outward_lathe(p, (0, radius, 0), profile, segments=16, uv=t.uv("orb", inset=2), color=(255,255,255))
    p.begin_material(p.material("cord_body"))
    solid_cylinder(p, (0, .371, 0), .020, .026, segments=8, uv=t.uv("rose"), color=(255,255,255))
    solid_cylinder(p, (0, .393, 0), .004, .387, segments=6, uv=t.uv("cord"), color=(100,100,95))
    solid_cylinder(p, (0, .78, 0), .05, .02, segments=10, uv=t.uv("rose"), color=(215,207,192))
    p.add_note("380mm opal globe with faceted closed shell, dark cord and ceiling rose; map-owned point lighting")


PROPS = {
    "home:ball_light": ball_light,
    "home:wall_switch": domestic_remade.switch,
    "home:crt_tv": domestic_remade.crt,
    **home_remade.PROPS,
}
