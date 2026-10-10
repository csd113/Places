#!/usr/bin/env python3
"""Build B17: yellow/turquoise tropical fish with dorsal and forked tail fins.

The authored ``swim`` loop moves the tail, pectoral fins and a restrained local
body turn/bob. It never patrols beyond the bind lighting/culling envelope.
"""

from __future__ import annotations

import math

from build_beach_common import Animal, Clip, main
from rig import quat_mul_many, quat_rot_y, quat_rot_z

PALETTE = {
    "yellow": (246, 214, 67), "yellow_light": (252, 225, 93),
    "turquoise": (25, 166, 184), "blue": (29, 139, 173),
    "fin": (32, 171, 190), "fin_light": (72, 191, 196),
    "eye": (24, 43, 51), "glint": (249, 244, 218),
}


def build():
    a = Animal(PALETTE)
    a.joint("root", (0, 0, 0))
    a.joint("body", (0, .17, 0), "root")
    a.joint("tail", (0, .158, -.222), "body")
    a.joint("dorsal", (0, .279, -.047), "body")
    # Elliptical eight-sided cross-section rings make the body full rather
    # than a flat icon. Its yellow snout and clean middle band are real regions
    # on the same fitted atlas, never overlapping decals.
    profile = [(-.236, .025, .040), (-.176, .071, .104),
               (-.090, .093, .132), (-.034, .097, .137),
               (.013, .096, .134), (.076, .085, .119),
               (.145, .059, .085), (.210, .024, .044)]
    vertices = []
    for z, rx, ry in profile:
        for i in range(8):
            angle = math.tau * i / 8
            vertices.append((rx * math.cos(angle), .167 + ry * math.sin(angle), z))
    segments = 8
    for row in range(len(profile) - 1):
        region = ("yellow" if row in (2, 5, 6) else
                  "yellow_light" if row == 3 else "turquoise")
        for i in range(segments):
            nxt = (i + 1) % segments
            indices = (row * segments + i, row * segments + nxt,
                       (row + 1) * segments + nxt, (row + 1) * segments + i)
            points = [vertices[index] for index in indices]
            for tri in ((points[0], points[1], points[2]), (points[0], points[2], points[3])):
                middle = tuple(sum(p[k] for p in tri) / 3 for k in range(3))
                a.triangle(tri, region, "body", (middle[0], middle[1] - .167, 0))
    # Tapered front/back caps use single centre points and no zero triangles.
    for row, tip, normal, region in ((0, (0, .167, -.253), (0, 0, -1), "turquoise"),
                                     (len(profile) - 1, (0, .158, .242), (0, 0, 1), "yellow_light")):
        for i in range(segments):
            a.triangle((tip, vertices[row * segments + i],
                        vertices[row * segments + (i + 1) % segments]), region, "body", normal)
    for s, side in ((1, "l"), (-1, "r")):
        a.ellipsoid((s * .070, .205, .122), (.012, .017, .016), "eye", "body", segments=6, rings=2)
        a.ellipsoid((s * .079, .212, .128), (.003, .004, .004), "glint", "body", segments=5, rings=1)
        a.joint(f"pectoral_{side}", (s * .080, .118, .052), "body")
        a.prism([(s * .077, .133, .065), (s * .147, .072, -.052),
                 (s * .088, .067, -.088)], (0, 0, 1), .012, "fin", f"pectoral_{side}")
    # Two joined dorsal sails preserve the distinctive yellow leading ray and
    # turquoise crown, and the smaller underside fin stays clear of the floor.
    a.prism([(0, .270, .050), (0, .386, .010), (0, .376, -.049),
             (0, .280, -.057)], (1, 0, 0), .017, "yellow", "dorsal")
    a.prism([(0, .284, -.050), (0, .378, -.049), (0, .350, -.149),
             (0, .267, -.180)], (1, 0, 0), .017, "fin", "dorsal")
    a.prism([(0, .051, -.045), (0, .017, -.072),
             (0, .047, -.169)], (1, 0, 0), .015, "blue", "body")
    # Forked tail: two closed lobes meet a short peduncle; the centre notch
    # remains an actual opening when viewed from either side.
    a.segment((0, .167, -.208), (0, .167, -.271), .019, "turquoise", "tail", sides=6,
              end_radius=.014)
    a.prism([(0, .170, -.255), (0, .306, -.396),
             (0, .182, -.347), (0, .153, -.282)],
            (1, 0, 0), .025, "blue", "tail")
    a.prism([(0, .163, -.255), (0, .147, -.282),
             (0, .149, -.347), (0, .009, -.396)],
            (1, 0, 0), .025, "fin", "tail")
    return a


def clips(a):
    swim = Clip("swim", duration=1.2, loop=True, kind="swim")
    count = 72
    for i in range(count + 1):
        phase = i / count if i < count else 0.0
        time = swim.duration * i / count
        wave = math.sin(math.tau * phase)
        tail_wave = math.sin(math.tau * phase + .35)
        swim.joint_translation(a.rig, "root", time, (0, .007 * wave, 0))
        swim.joint_rotation(a.rig, "body", time,
                            quat_mul_many(quat_rot_y(3.5 * wave), quat_rot_z(1.5 * wave)))
        swim.joint_rotation(a.rig, "tail", time, quat_rot_y(17 * tail_wave))
        swim.joint_rotation(a.rig, "dorsal", time, quat_rot_z(2.5 * wave))
        for s, side in ((1, "l"), (-1, "r")):
            swim.joint_rotation(a.rig, f"pectoral_{side}", time, quat_rot_z(s * 9 * wave))
    return [swim]


if __name__ == "__main__":
    raise SystemExit(main("fish", PALETTE, build, clips))
