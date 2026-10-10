#!/usr/bin/env python3
"""Build B16: broad coral crab with stalk eyes, eight legs and two pincers.

The leg pairs alternate planted/swing arcs in the ``walk`` loop; the stride
reference is measured from its ground-contact sweep. ``idle`` sways stalk eyes
and opens each claw slightly. Local motion does not translate the root.
"""

from __future__ import annotations

import math

from build_beach_common import Animal, Clip, main
from rig import quat_mul_many, quat_rot_x, quat_rot_y, quat_rot_z

PALETTE = {
    "coral": (237, 91, 66), "top": (248, 112, 77),
    "underside": (206, 71, 54), "leg": (223, 78, 54),
    "claw": (246, 101, 69), "stalk": (211, 70, 49),
    "eye": (34, 37, 39), "glint": (248, 235, 199),
}
WALK_SECONDS = .8
DUTY = .60
HIP_EXTENT = .222
SWING_DEGREES = 10.0
REFERENCE_SPEED_MPS = 2 * HIP_EXTENT * math.sin(math.radians(SWING_DEGREES)) / (DUTY * WALK_SECONDS)


def build():
    a = Animal(PALETTE)
    a.joint("root", (0, 0, 0))
    a.joint("body", (0, .190, -.012), "root")
    # A wide eight-sided carapace with a shallow pointed rim, instead of a
    # sphere. The cream board depicts clean coral facets and a wide flat body.
    a.ellipsoid((0, .190, -.010), (.241, .104, .180), "coral", "body", segments=8, rings=2)
    a.ellipsoid((0, .222, -.015), (.219, .072, .155), "top", "body", segments=8, rings=1)
    a.ellipsoid((0, .135, -.010), (.183, .037, .129), "underside", "body", segments=6, rings=1)
    for s, side in ((1, "l"), (-1, "r")):
        a.joint(f"eye_stalk_{side}", (s * .090, .246, .109), "body")
        a.segment((s * .090, .246, .109), (s * .096, .345, .120), .013,
                  "stalk", f"eye_stalk_{side}", sides=5, end_radius=.011)
        a.ellipsoid((s * .096, .348, .120), (.028, .033, .028), "coral",
                    f"eye_stalk_{side}", segments=6, rings=2)
        a.ellipsoid((s * .096, .353, .141), (.019, .021, .013), "eye",
                    f"eye_stalk_{side}", segments=6, rings=2)
        a.ellipsoid((s * .090, .361, .153), (.005, .006, .004), "glint",
                    f"eye_stalk_{side}", segments=5, rings=1)
        for leg, z in enumerate((.097, .035, -.039, -.108)):
            hip = (s * .185, .164, z)
            knee = (s * (.300 + .008 * (leg % 2)), .132, z - .028)
            toe = (s * .407, .013, z + .023)
            name = f"leg_{side}_{leg + 1:02d}"
            a.joint(name, hip, "root")
            a.segment(hip, knee, .026, "leg", name, sides=5, end_radius=.020)
            a.segment(knee, toe, .020, "coral", name, sides=5, end_radius=.010)
            a.ellipsoid(toe, (.010, .013, .015), "leg", name, segments=5, rings=1)
        a.joint(f"claw_{side}", (s * .174, .183, .121), "body")
        a.joint(f"pincer_{side}", (s * .333, .142, .271), f"claw_{side}")
        a.segment((s * .174, .183, .121), (s * .272, .155, .183), .036,
                  "leg", f"claw_{side}", sides=5, end_radius=.031)
        a.segment((s * .272, .155, .183), (s * .329, .142, .275), .030,
                  "coral", f"claw_{side}", sides=5, end_radius=.038)
        a.ellipsoid((s * .333, .142, .277), (.052, .058, .060), "claw",
                    f"pincer_{side}", segments=6, rings=1)
        # Two triangular curved fingers remain visibly separated. Outer finger
        # belongs to the palm, inner finger has its own articulation/socket.
        outer = [(s * .365, .145, .280), (s * .391, .135, .351),
                 (s * .344, .130, .403), (s * .354, .133, .350),
                 (s * .327, .139, .312)]
        # Split the hooked outline into convex wedges to keep all caps closed
        # with verified outward winding and no concave fan triangulation.
        a.prism(outer[:3] + [outer[4]], (0, 1, 0), .044, "claw", f"pincer_{side}")
        a.joint(f"finger_{side}", (s * .318, .139, .291), f"pincer_{side}")
        a.prism([(s * .310, .142, .285), (s * .285, .135, .349),
                 (s * .324, .130, .394), (s * .320, .134, .341)],
                (0, 1, 0), .033, "coral", f"finger_{side}")
    # A fine coral mouth ridge keeps the front readable below the stalk eyes.
    a.segment((-.060, .186, .169), (.060, .186, .169), .011,
              "underside", "body", sides=4)
    return a


def clips(a):
    idle = Clip("idle", duration=2.4, loop=True, kind="idle")
    walk = Clip("walk", duration=WALK_SECONDS, loop=True, kind="walk",
                reference_speed_mps=REFERENCE_SPEED_MPS)
    for clip, count in ((idle, 72), (walk, 80)):
        for i in range(count + 1):
            phase = i / count if i < count else 0.0
            time = clip.duration * i / count
            wave = math.sin(math.tau * phase)
            clip.joint_translation(a.rig, "body", time, (0, .0015 * (1 + wave), 0))
            for s, side in ((1, "l"), (-1, "r")):
                clip.joint_rotation(a.rig, f"eye_stalk_{side}", time,
                                    quat_rot_z(s * 2.5 * wave))
                clip.joint_rotation(a.rig, f"claw_{side}", time,
                                    quat_rot_y(s * (2.2 * wave)))
                clip.joint_rotation(a.rig, f"pincer_{side}", time,
                                    quat_rot_x(2.5 * wave))
                clip.joint_rotation(a.rig, f"finger_{side}", time,
                                    quat_rot_y(s * (3 + 3 * wave)))
                for leg in range(4):
                    name = f"leg_{side}_{leg + 1:02d}"
                    if clip is walk:
                        p = (phase + .5 * ((leg + (s < 0)) % 2)) % 1
                        if p < DUTY:
                            yaw = -SWING_DEGREES + 2 * SWING_DEGREES * p / DUTY
                            lift = 0
                        else:
                            swing = (p - DUTY) / (1 - DUTY)
                            yaw = SWING_DEGREES * math.cos(math.pi * swing)
                            lift = 5.0 * math.sin(math.pi * swing)
                        rotation = quat_mul_many(quat_rot_y(s * yaw), quat_rot_z(s * lift))
                    else:
                        rotation = quat_rot_y(0)
                    clip.joint_rotation(a.rig, name, time, rotation)
    return [idle, walk]


if __name__ == "__main__":
    raise SystemExit(main("crab", PALETTE, build, clips))
