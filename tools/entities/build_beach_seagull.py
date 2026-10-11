#!/usr/bin/env python3
"""Build B15: white/grey Beach gull, dark wing tips and yellow bill/feet.

The spread bind pose measures the whole flying silhouette. ``idle`` folds the
wings against the flanks; ``fly`` articulates the wings and tucks the legs.
Both are in-place loops, with every posed vertex inside measured runtime
culling/lighting bounds. Default export reads committed PNG artwork.
"""

from __future__ import annotations

import math

from build_beach_common import Animal, Clip, main
from rig import quat_mul_many, quat_rot_x, quat_rot_y, quat_rot_z

PALETTE = {
    "white": (236, 234, 226), "grey": (166, 178, 193),
    "grey_light": (192, 200, 210), "tip": (64, 72, 90),
    "bill": (246, 191, 57), "feet": (231, 174, 43),
    "eye": (29, 36, 43), "glint": (252, 250, 239),
}


def build():
    a = Animal(PALETTE)
    a.joint("root", (0, 0, 0))
    a.joint("body", (0, .405, -.03), "root")
    a.joint("neck", (0, .52, .12), "body")
    a.joint("head", (0, .63, .17), "neck")
    a.joint("tail", (0, .395, -.26), "body")
    a.ellipsoid((0, .40, -.065), (.125, .155, .24), "white", "body", rings=3)
    a.ellipsoid((0, .52, .12), (.071, .125, .091), "white", "neck", segments=7, rings=2)
    a.ellipsoid((0, .648, .17), (.078, .099, .089), "white", "head", segments=8, rings=3)
    # The long tapered yellow bill has a visibly closed lower edge and pointed
    # tip, instead of a rectangular beak. The reference gull faces +Z.
    a.polyhedron([(-.038, .651, .242), (.038, .651, .242),
                  (-.032, .616, .242), (.032, .616, .242),
                  (0, .624, .416)],
                 [(0, 1, 4), (2, 3, 4), (0, 2, 4), (1, 3, 4), (0, 1, 3, 2)],
                 "bill", "head")
    for s, side in ((1, "l"), (-1, "r")):
        a.ellipsoid((s * .072, .666, .204), (.013, .015, .012), "eye", "head",
                    segments=6, rings=2)
        a.ellipsoid((s * .081, .671, .209), (.004, .004, .004), "glint", "head",
                    segments=5, rings=1)
        shoulder = (s * .084, .458, -.06)
        a.joint(f"wing_{side}", shoulder, "body")
        a.joint(f"wing_tip_{side}", (s * .385, .435, -.11), f"wing_{side}")
        # The six-corner proximal wing is broad at its shoulder and sweeps
        # back into a pointed, charcoal outer primary. Both are closed shells.
        a.prism([(s * .084, .458, .037), (s * .245, .46, .045),
                 (s * .398, .435, -.043), (s * .392, .424, -.185),
                 (s * .18, .442, -.205), (s * .076, .452, -.119)],
                (0, 1, 0), .023, "grey", f"wing_{side}")
        a.prism([(s * .377, .438, -.046), (s * .497, .417, -.093),
                 (s * .548, .400, -.189), (s * .403, .418, -.225)],
                (0, 1, 0), .018, "tip", f"wing_tip_{side}")
        # Pale coverts sit on the upper proximal wing, giving the layered
        # light/grey folded silhouette seen in the board without feather noise.
        a.prism([(s * .090, .474, .001), (s * .211, .474, .012),
                 (s * .348, .455, -.072), (s * .18, .464, -.131)],
                (0, 1, 0), .012, "grey_light", f"wing_{side}")
        hip = (s * .051, .274, .029)
        a.joint(f"leg_{side}", hip, "root")
        a.joint(f"foot_{side}", (s * .053, .028, .032), f"leg_{side}")
        a.segment(hip, (s * .053, .028, .032), .012, "feet", f"leg_{side}", sides=5,
                  end_radius=.009)
        a.ellipsoid((s * .053, .018, .047), (.021, .018, .035), "feet", f"foot_{side}",
                    segments=5, rings=1)
        for spread in (-1, 0, 1):
            a.segment((s * .053, .016, .061),
                      (s * .053 + spread * .027, .009, .106 - abs(spread) * .014),
                      .007, "feet", f"foot_{side}", sides=4, end_radius=.004)
        a.segment((s * .053, .014, .037), (s * .053, .010, .011), .006,
                  "feet", f"foot_{side}", sides=4, end_radius=.004)
    a.prism([(-.052, .416, -.232), (.052, .416, -.232),
             (.069, .402, -.412), (0, .395, -.466), (-.069, .402, -.412)],
            (0, 1, 0), .018, "grey_light", "tail")
    a.prism([(-.069, .404, -.396), (.069, .404, -.396),
             (0, .396, -.47)], (0, 1, 0), .016, "tip", "tail")
    return a


def clips(a):
    idle = Clip("idle", duration=3.0, loop=True, kind="idle")
    fly = Clip("fly", duration=.8, loop=True, kind="airborne")
    for clip, count in ((idle, 60), (fly, 48)):
        for i in range(count + 1):
            phase = i / count if i < count else 0.0
            time = clip.duration * i / count
            wave = math.sin(math.tau * phase)
            clip.joint_rotation(a.rig, "head", time,
                                quat_mul_many(quat_rot_y(4.5 * wave), quat_rot_x(2 * wave)))
            clip.joint_rotation(a.rig, "neck", time, quat_rot_x(1.5 * wave))
            for s, side in ((1, "l"), (-1, "r")):
                if clip is idle:
                    wing = quat_mul_many(quat_rot_y(s * 78), quat_rot_z(s * (-7 + wave)))
                    tip = quat_rot_y(s * 5)
                    leg = quat_rot_x(0)
                    foot = quat_rot_x(0)
                else:
                    # Positive left Z raises the +X wing; negative right Z
                    # raises its mirror. The asymmetric up/down phase is clear
                    # at normal gameplay scale without leaving measured bounds.
                    wing = quat_rot_z(s * (22 * wave - 2))
                    tip = quat_mul_many(quat_rot_z(s * (5 * wave)), quat_rot_y(s * -3))
                    leg = quat_rot_x(-68)
                    foot = quat_rot_x(30)
                clip.joint_rotation(a.rig, f"wing_{side}", time, wing)
                clip.joint_rotation(a.rig, f"wing_tip_{side}", time, tip)
                clip.joint_rotation(a.rig, f"leg_{side}", time, leg)
                clip.joint_rotation(a.rig, f"foot_{side}", time, foot)
            clip.joint_rotation(a.rig, "tail", time, quat_rot_x(2.5 * wave))
    return [idle, fly]


if __name__ == "__main__":
    raise SystemExit(main("seagull", PALETTE, build, clips))
