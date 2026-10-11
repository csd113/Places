"""Reusable Beach authoring pieces, not a composed demonstration level.

These return existing v3 arrays. A map owner chooses placement, curved-strip
layout and containment. Decorative terrain GLBs never supply walking support.
"""
from __future__ import annotations

import math


from tools.props.parts.beach_coast import coast_z, cove_foam_profile, cove_foam_bounds


def shore_segment(x: float, sea_z: float, shore_z: float, width: float) -> dict:
    """A north-facing dry/wade/swim strip with real floor and gentle ramps.

    The containing room starts at the -2.2 m seabed, including underwater
    entity lighting anchors. Dry support is raised to world Y=0; water is -.12.
    Adjacent strips can have different shore_z to follow the visual coast.
    Water ends over the entry slope before it meets dry sand. Dock floor
    regions must be placed outside these ramps to preserve one floor owner.
    """
    if not all(math.isfinite(value) for value in (x, sea_z, shore_z, width)):
        raise ValueError("shore coordinates and width must be finite")
    if width <= .04 or shore_z - sea_z <= 4:
        raise ValueError("shore strip needs width over four centimetres and more than four metres of sea")
    # The implemented ramp run is its longer dimension; there is no axis
    # override. Narrow adjoining strips keep both ramps running north/south.
    count = math.ceil(width / 1.8)
    strip_width = width / count
    # Inclusive room edges otherwise let a neighbour's raised coast own a
    # water corner when adjoining strips have different shore positions.
    water_inset = .02  # Clear RoomDef.contains' one-centimetre edge tolerance.
    water_x, water_width = x+water_inset, width-2*water_inset
    ramps = []
    for index in range(count):
        for z, depth, offset, rise in ((shore_z - 4, 2, 0, 1.6),
                                      (shore_z - 2, 4, 1.6, .6)):
            ramps.append({"x": x + index * strip_width, "z": z,
                          "width": strip_width, "depth": depth,
                          "offset_y": offset, "rise": rise,
                          "material": "beach:sand_01", "edge_material": "beach:sand_01"})
    return {
        "rooms": [{"x": x, "z": sea_z, "width": width, "depth": shore_z + 6 - sea_z,
                   "height": 14.2, "floor_y": -2.2, "material": "beach:sand_01",
                   "ceiling": {"kind": "open"}}],
        "floor_regions": [{"x": x, "z": shore_z + 2, "width": width, "depth": 4,
                           "offset_y": 2.2, "material": "beach:sand_01",
                           "edge_material": "beach:sand_01"}],
        "ramps": ramps,
        "water": [{"x": water_x, "z": sea_z, "width": water_width, "depth": shore_z - 4 - sea_z,
                   "surface_y": -.12, "bottom_y": -2.2, "material": "beach:water_deep_01",
                   "opacity": .62, "attenuation_per_metre": .10, "swimming": True},
                  {"x": water_x, "z": shore_z - 4, "width": water_width, "depth": 4.9,
                   "surface_y": -.12, "bottom_y": -2.2, "material": "beach:water_shallow_01",
                   "opacity": .34, "attenuation_per_metre": .10, "swimming": True}],
    }


def daylight() -> dict:
    """Sky and sun share the panorama's +45 degree bearing/elevation."""
    return {
        "sky": {"texture": "beach:tex_sky_day_01", "brightness": 1,
                "ambient": .32, "ambient_color": [.58, .75, 1]},
        "global_illuminators": [{"id": "beach_sun", "kind": "directional",
                                 "direction": [-.5, -math.sqrt(.5), .5],
                                 "color": [1, .94, .82], "intensity": .9,
                                 "angular_size_degrees": .8, "cast_shadows": True,
                                 "enabled": True, "bake": True}],
    }
