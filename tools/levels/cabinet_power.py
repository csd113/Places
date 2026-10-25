"""Shared authoring for a cabinet's screen power and modest local spill.

Uses measured cabinet profiles, keeping future placements on the same light
owner contract. Decorative fixtures belong to their independently authored owner.
"""
from __future__ import annotations


def cabinet_power(profile, color, *, enabled=True, emission_scale=1.0,
                  switchable=False):
    """Return existing level fields; no runtime asset generation or new format.

    One short-range source represents the small CRT and lightbox, rather than
    overlapping room-scale coloured pools. Width scales the common CRT family's
    output; all current screens share their approximately 0.43 m sloping height.
    Standby scales the visible emission and baked source by the same amount.
    Uncontrolled fully powered duplicates retain their existing draw merging.
    """
    fields = dict(lights=[dict(shape="point", offset=profile["light_offsets"][0],
        color=list(color), intensity=round(.14 * profile["size"][0] / .82 * emission_scale, 6),
        range=1.8)])
    if not enabled or emission_scale != 1.0 or switchable:
        fields["components"] = [dict(component="light", enabled=enabled,
            switchable=switchable, emission_scale=emission_scale)]
    return fields
