"""Primitive mesh builder shared by every prop in the pack.

The builder is deliberately small: boxes, low-segment cylinders and quads are
enough for the whole pack, and keeping to them guarantees the coherent
PS2-era silhouette the pack aims for.  It also enforces the pack's hard
conventions while building, so a broken prop fails loudly instead of shipping:

* 1 unit = 1 metre, Y up, floors rest on y = 0;
* bbox is horizontally centred on the origin and its minimum Y is 0;
* the bounding box matches the catalogue ``size`` within a small tolerance;
* ``+Z`` is the prop's front (fridge doors, TV screen, vending machine panel);
* triangles face outwards (counter-clockwise seen from outside);
* one texture, one material, no alpha.

Each primitive also records a coarse "proxy" part (box/cylinder + colour) that
is exported for the level editor's 3D preview, so the editor always shows a
lightweight approximation *derived from the real asset* rather than a
hand-maintained duplicate.
"""

from __future__ import annotations

import math
from typing import Dict, Iterable, Sequence

from palette import shade
from tex import Color, Texture

# Per-face shading multipliers, mirroring the game's box-prop shading
# (src/render.rs `PROP_FACE_SHADES`) so hand-built meshes and fallback boxes
# respond to light in the same way: top brightest, bottom darkest.
FACE_SHADE = {
    "+y": 1.00,
    "-y": 0.62,
    "+z": 0.90,
    "-z": 0.80,
    "-x": 0.74,
    "+x": 0.86,
}

FACE_KEYS = ("+x", "-x", "+y", "-y", "+z", "-z")

UV = tuple[float, float, float, float]  # u0, v0, u1, v1


def _rotate(point: tuple[float, float, float], degrees: tuple[float, float, float]) -> tuple[float, float, float]:
    """Applies an XYZ Euler rotation (degrees) to a point."""
    x, y, z = point
    rx, ry, rz = (math.radians(value) for value in degrees)
    if rx:
        cos, sin = math.cos(rx), math.sin(rx)
        y, z = y * cos - z * sin, y * sin + z * cos
    if ry:
        cos, sin = math.cos(ry), math.sin(ry)
        x, z = x * cos + z * sin, -x * sin + z * cos
    if rz:
        cos, sin = math.cos(rz), math.sin(rz)
        x, y = x * cos - y * sin, x * sin + y * cos
    return (x, y, z)


class Mesh:
    """Triangle soup with positions, vertex colours, UVs and 16-bit indices."""

    def __init__(self) -> None:
        self.positions: list[tuple[float, float, float]] = []
        self.colors: list[tuple[int, int, int]] = []
        self.uvs: list[tuple[float, float]] = []
        self.indices: list[int] = []
        # Coarse parts for the editor's 3D proxy geometry.
        self.parts: list[dict] = []

    # ------------------------------------------------------------ primitives

    def quad(
        self,
        p0,
        p1,
        p2,
        p3,
        uv: UV | Sequence[tuple[float, float]],
        color: Color,
        shade_mult: float = 1.0,
        ao: float = 1.0,
    ) -> None:
        """Adds a quad with corners in counter-clockwise order (seen from the front)."""
        if isinstance(uv, (list, tuple)) and len(uv) == 4 and isinstance(uv[0], (list, tuple)):
            uvs = [(float(pair[0]), float(pair[1])) for pair in uv]  # type: ignore[index]
        else:
            u0, v0, u1, v1 = uv  # type: ignore[misc]
            uvs = [(u0, v1), (u1, v1), (u1, v0), (u0, v0)]
        base = 0
        for point in (p0, p1, p2, p3):
            self._add_vertex(point, color, uvs[base], shade_mult, ao)
            base += 1
        start = len(self.positions) - 4
        self.indices += [start, start + 1, start + 2, start, start + 2, start + 3]

    def triangle(self, p0, p1, p2, uvs, color: Color, shade_mult: float = 1.0, ao: float = 1.0) -> None:
        for point, uv in zip((p0, p1, p2), uvs):
            self._add_vertex(point, color, uv, shade_mult, ao)
        start = len(self.positions) - 3
        self.indices += [start, start + 1, start + 2]

    def box(
        self,
        center: Sequence[float],
        size: Sequence[float],
        uv: UV | Dict[str, UV] | None = None,
        color: Color = (255, 255, 255),
        colors: Dict[str, Color] | None = None,
        shade: bool = True,
        rotation: Sequence[float] | None = None,
        ao: float = 1.0,
        proxy: bool = True,
    ) -> None:
        """Axis-aligned box, optionally rotated about its centre.

        ``uv`` may be one rectangle shared by every face or a per-face mapping
        keyed by ``+x/-x/+y/-y/+z/-z``; ``colors`` likewise overrides per face.
        """
        cx, cy, cz = center
        hx, hy, hz = (dimension * 0.5 for dimension in size)
        default_uv = uv if isinstance(uv, tuple) else None
        face_uvs: Dict[str, UV] = {}
        if isinstance(uv, dict):
            face_uvs = uv
        elif default_uv is not None:
            face_uvs = {key: default_uv for key in FACE_KEYS}

        # Corner signs per face, in counter-clockwise order seen from outside.
        faces = {
            "+x": ((1, -1, -1), (1, -1, 1), (1, 1, 1), (1, 1, -1)),
            "-x": ((-1, -1, 1), (-1, -1, -1), (-1, 1, -1), (-1, 1, 1)),
            "+y": ((-1, 1, -1), (-1, 1, 1), (1, 1, 1), (1, 1, -1)),
            "-y": ((-1, -1, -1), (1, -1, -1), (1, -1, 1), (-1, -1, 1)),
            "+z": ((-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)),
            "-z": ((1, -1, -1), (-1, -1, -1), (-1, 1, -1), (1, 1, -1)),
        }

        for key, signs in faces.items():
            uv_rect = face_uvs.get(key)
            if uv_rect is None:
                continue
            face_color = colors.get(key, color) if colors else color
            multiplier = FACE_SHADE[key] if shade else 1.0
            points = []
            for sign in signs:
                local = (sign[0] * hx, sign[1] * hy, sign[2] * hz)
                if rotation:
                    local = _rotate(local, tuple(rotation))
                points.append((cx + local[0], cy + local[1], cz + local[2]))
            self.quad(*points, uv=uv_rect, color=face_color, shade_mult=multiplier, ao=ao)

        if proxy:
            self.parts.append(
                {
                    "shape": "box",
                    "center": [round(float(value), 4) for value in (cx, cy, cz)],
                    "size": [round(float(value), 4) for value in size],
                    "rotation": [round(float(value), 2) for value in (rotation or (0.0, 0.0, 0.0))],
                    "color": _hex(color),
                }
            )

    def cylinder(
        self,
        base: Sequence[float],
        radius: float,
        height: float,
        segments: int = 8,
        axis: str = "y",
        uv: UV | None = None,
        side_uv: UV | None = None,
        cap_uv: UV | None = None,
        color: Color = (255, 255, 255),
        top_color: Color | None = None,
        shades: bool = True,
        taper: float = 1.0,
        rotation: float = 0.0,
        shade_override: float = 1.0,
        proxy: bool = True,
        bottom: bool = False,
        ao: float = 1.0,
    ) -> None:
        """Low-segment cylinder (or tapered cone with ``taper``).

        ``base`` is the centre of the bottom cap; for horizontal axes the
        cylinder extends along that axis (``+x``/``+z``), which keeps faucets,
        handles and pipes cheap.
        """
        if segments < 3:
            raise ValueError("a cylinder needs at least 3 segments")
        rect = uv or side_uv or (0.0, 0.0, 1.0, 1.0)
        cap_rect = cap_uv or rect
        top_rect = cap_uv or rect
        bx, by, bz = (float(value) for value in base)

        def point_at(angle: float, along: float, radial: float) -> tuple[float, float, float]:
            cos, sin = math.cos(angle), math.sin(angle)
            ox, oz = cos * radial, sin * radial
            if axis == "y":
                return (bx + ox, by + along, bz + oz)
            if axis == "x":
                return (bx + along, by + oz, bz + ox)
            return (bx + ox, by + oz, bz + along)

        u0, v0, u1, v1 = rect
        for index in range(segments):
            a0 = (index / segments) * math.tau + rotation
            a1 = ((index + 1) / segments) * math.tau + rotation
            t0 = index / segments
            t1 = (index + 1) / segments
            bottom0 = point_at(a0, 0.0, radius)
            bottom1 = point_at(a1, 0.0, radius)
            top0 = point_at(a0, height, radius * taper)
            top1 = point_at(a1, height, radius * taper)
            multiplier = 1.0 if not shades else (0.72 + 0.28 * (0.5 + 0.5 * math.cos(a0 - 0.9)))
            multiplier *= shade_override
            color_a = top_color or color
            self.quad(
                bottom0,
                bottom1,
                top1,
                top0,
                (
                    (u0 + (u1 - u0) * t0, v1),
                    (u0 + (u1 - u0) * t1, v1),
                    (u0 + (u1 - u0) * t1, v0),
                    (u0 + (u1 - u0) * t0, v0),
                ),
                color_a,
                shade_mult=multiplier,
                ao=ao,
            )
        # Caps: a fan of triangles; the top cap is usually the only visible one.
        cap_color = top_color or color
        for index in range(segments):
            a0 = (index / segments) * math.tau + rotation
            a1 = ((index + 1) / segments) * math.tau + rotation
            self.triangle(
                point_at(0.0, height, 0.0),
                point_at(a0, height, radius * taper),
                point_at(a1, height, radius * taper),
                _cap_uvs(cap_rect),
                cap_color,
                shade_mult=(1.0 if not shades else 0.96) * shade_override,
                ao=ao,
            )
            if bottom:
                self.triangle(
                    point_at(0.0, 0.0, 0.0),
                    point_at(a1, 0.0, radius),
                    point_at(a0, 0.0, radius),
                    _cap_uvs(top_rect),
                    color,
                    shade_mult=(1.0 if not shades else 0.62) * shade_override,
                    ao=ao,
                )

        if proxy:
            self.parts.append(
                {
                    "shape": "cylinder",
                    "axis": axis,
                    "base": [round(float(value), 4) for value in (bx, by, bz)],
                    "radius": round(float(radius), 4),
                    "height": round(float(height), 4),
                    "segments": int(segments),
                    "taper": round(float(taper), 3),
                    "color": _hex(color),
                }
            )

    def tube(self, start, end, radius: float, segments: int = 6, uv: UV | None = None,
             color: Color = (255, 255, 255), proxy: bool = True, ao: float = 1.0) -> None:
        """Straight pipe between two arbitrary points (faucets, lamp stems)."""
        sx, sy, sz = (float(value) for value in start)
        ex, ey, ez = (float(value) for value in end)
        dx, dy, dz = ex - sx, ey - sy, ez - sz
        length = math.sqrt(dx * dx + dy * dy + dz * dz)
        if length <= 1e-6:
            raise ValueError("tube endpoints must differ")
        axis = max(((abs(dx), "x"), (abs(dy), "y"), (abs(dz), "z")))[1]
        # Build an axis-aligned tube then rotate it onto the segment direction.
        hx, hy, hz = dx / length, dy / length, dz / length
        # Rotation angles that map the chosen axis onto (hx, hy, hz).
        if axis == "y":
            angle_z = math.degrees(math.atan2(-hx, hy))
            angle_x = math.degrees(math.atan2(hz, math.sqrt(hx * hx + hy * hy)))
            rotation = (angle_x, 0.0, -angle_z)
        elif axis == "x":
            angle_z = math.degrees(math.atan2(hy, hx))
            angle_y = math.degrees(math.atan2(-hz, math.sqrt(hx * hx + hy * hy)))
            rotation = (0.0, angle_y, angle_z)
        else:
            angle_x = math.degrees(math.atan2(-hy, hz))
            angle_y = math.degrees(math.atan2(hx, math.sqrt(hy * hy + hz * hz)))
            rotation = (angle_x, -angle_y, 0.0)

        center = ((sx + ex) / 2, (sy + ey) / 2, (sz + ez) / 2)
        rot = rotation
        local_base = (0.0, -length / 2, 0.0)
        rotated = _rotate(local_base, rot)
        base = (center[0] + rotated[0], center[1] + rotated[1], center[2] + rotated[2])

        # Emit the side wall manually so arbitrary directions stay simple.
        rect = uv or (0.0, 0.0, 1.0, 1.0)
        u0, v0, u1, v1 = rect
        # Orthonormal basis perpendicular to the segment.
        ref = (0.0, 1.0, 0.0) if abs(hy) < 0.9 else (1.0, 0.0, 0.0)
        ux = _cross(ref, (hx, hy, hz))
        ux = _normalize(ux)
        uy = _cross((hx, hy, hz), ux)
        for index in range(segments):
            a0 = (index / segments) * math.tau
            a1 = ((index + 1) / segments) * math.tau
            t0, t1 = index / segments, (index + 1) / segments
            p0 = _ring_point(start, ux, uy, a0, radius)
            p1 = _ring_point(start, ux, uy, a1, radius)
            p2 = _ring_point(end, ux, uy, a1, radius)
            p3 = _ring_point(end, ux, uy, a0, radius)
            multiplier = 0.74 + 0.26 * (0.5 + 0.5 * math.cos(a0 - 1.1))
            self.quad(
                p0, p1, p2, p3,
                (
                    (u0 + (u1 - u0) * t0, v1),
                    (u0 + (u1 - u0) * t1, v1),
                    (u0 + (u1 - u0) * t1, v0),
                    (u0 + (u1 - u0) * t0, v0),
                ),
                color,
                shade_mult=multiplier,
                ao=ao,
            )
        for point, flip in ((start, False), (end, True)):
            for index in range(segments):
                a0 = (index / segments) * math.tau
                a1 = ((index + 1) / segments) * math.tau
                ring0 = _ring_point(point, ux, uy, a0, radius)
                ring1 = _ring_point(point, ux, uy, a1, radius)
                center = tuple(float(value) for value in point)
                if flip:
                    self.triangle(center, ring0, ring1, _cap_uvs(rect), shade(color, 0.94), shade_mult=0.95, ao=ao)
                else:
                    self.triangle(center, ring1, ring0, _cap_uvs(rect), shade(color, 0.7), shade_mult=0.7, ao=ao)

        if proxy:
            self.parts.append(
                {
                    "shape": "tube",
                    "start": [round(sx, 4), round(sy, 4), round(sz, 4)],
                    "end": [round(ex, 4), round(ey, 4), round(ez, 4)],
                    "radius": round(radius, 4),
                    "color": _hex(color),
                }
            )

    def lathe(
        self,
        base: Sequence[float],
        profile: Sequence[Sequence[float]],
        segments: int = 8,
        axis: str = "y",
        uv: UV | None = None,
        cap_uv: UV | None = None,
        color: Color = (255, 255, 255),
        shades: bool = True,
        cap_start: bool = True,
        cap_end: bool = True,
        rotation: float = 0.0,
        ellipse: Sequence[float] = (1.0, 1.0),
        proxy: bool = True,
        ao: float = 1.0,
    ) -> None:
        """Surface of revolution from a list of ``(along, radius)`` rings.

        The organic low-poly mass a box or cylinder cannot express: a cat's
        torso, head, muzzle, legs and paws are all lathes here. ``base`` is the
        point on the axis at ``along = 0`` and the ring plane is perpendicular
        to ``axis`` (``x`` → Y/Z, ``y`` → X/Z, ``z`` → X/Y). ``ellipse`` scales
        the two ring axes, so a torso can be broader than it is tall.

        UVs: ``u`` runs once around the rings (starting at angle ``rotation``)
        and ``v`` runs along the profile, first entry on the ``v1`` (bottom)
        edge of the region and last on the ``v0`` (top) edge, so the texture
        reads like a side-view photograph. For ``axis="z"`` with
        ``rotation=pi/2``: ``u=0`` is the top/back, ``u=0.25`` the left,
        ``u=0.5`` the bottom/belly and ``u=0.75`` the right side.
        """
        if segments < 3:
            raise ValueError("a lathe needs at least 3 segments")
        if len(profile) < 2:
            raise ValueError("a lathe needs at least 2 profile rings for a silhouette")
        bx, by, bz = (float(value) for value in base)
        e0, e1 = float(ellipse[0]), float(ellipse[1])
        u0, v0, u1, v1 = uv or (0.0, 0.0, 1.0, 1.0)

        def point(along: float, angle: float, radius: float) -> tuple[float, float, float]:
            cos, sin = math.cos(angle), math.sin(angle)
            if axis == "y":
                return (bx + cos * radius * e0, by + along, bz + sin * radius * e1)
            if axis == "x":
                return (bx + along, by + sin * radius * e1, bz + cos * radius * e0)
            return (bx + cos * radius * e0, by + sin * radius * e1, bz + along)

        ring_count = len(profile)
        rings: list[list[tuple[float, float, float]]] = []
        for along, radius in profile:
            rings.append(
                [point(float(along), (index / segments) * math.tau + rotation, float(radius))
                 for index in range(segments)]
            )

        for ring_index in range(ring_count - 1):
            lower = rings[ring_index]
            upper = rings[ring_index + 1]
            for index in range(segments):
                nxt = (index + 1) % segments
                angle_mid = ((index + 0.5) / segments) * math.tau + rotation
                multiplier = 1.0 if not shades else 0.72 + 0.28 * (0.5 + 0.5 * math.cos(angle_mid - 0.9))
                # Wrap the geometry index, but let the seam UV reach u1.
                # Returning to u0 here stretches the entire atlas across the last face.
                self.quad(
                    lower[index],
                    lower[nxt],
                    upper[nxt],
                    upper[index],
                    [
                        (u0 + (u1 - u0) * (index / segments), v1 + (v0 - v1) * (ring_index / (ring_count - 1))),
                        (u0 + (u1 - u0) * ((index + 1) / segments), v1 + (v0 - v1) * (ring_index / (ring_count - 1))),
                        (u0 + (u1 - u0) * ((index + 1) / segments), v1 + (v0 - v1) * ((ring_index + 1) / (ring_count - 1))),
                        (u0 + (u1 - u0) * (index / segments), v1 + (v0 - v1) * ((ring_index + 1) / (ring_count - 1))),
                    ],
                    color,
                    shade_mult=multiplier,
                    ao=ao,
                )

        rect = cap_uv or (u0, v0, u1, v1)
        first = profile[0]
        last = profile[-1]
        for index in range(segments):
            nxt = (index + 1) % segments
            angle0 = (index / segments) * math.tau + rotation
            angle1 = (nxt / segments) * math.tau + rotation
            if cap_start and float(first[1]) > 1e-4:
                self.triangle(
                    point(float(first[0]), 0.0, 0.0),
                    rings[0][nxt],
                    rings[0][index],
                    _cap_uvs(rect),
                    color,
                    shade_mult=(1.0 if not shades else 0.66),
                    ao=ao,
                )
            if cap_end and float(last[1]) > 1e-4:
                self.triangle(
                    point(float(last[0]), 0.0, 0.0),
                    rings[-1][index],
                    rings[-1][nxt],
                    _cap_uvs(rect),
                    color,
                    shade_mult=(1.0 if not shades else 0.97),
                    ao=ao,
                )
            del angle0, angle1

        if proxy:
            radii = [float(entry[1]) for entry in profile]
            self.parts.append(
                {
                    "shape": "cylinder",
                    "axis": axis,
                    "base": [round(bx, 4), round(by, 4), round(bz, 4)],
                    "radius": round(max(radii), 4),
                    "height": round(abs(float(last[0]) - float(first[0])), 4),
                    "segments": int(segments),
                    "taper": round(radii[-1] / max(1e-6, radii[0]), 3),
                    "color": _hex(color),
                }
            )

    def tube_path(
        self,
        points: Sequence[Sequence[float]],
        radii: Sequence[float] | float = 0.05,
        segments: int = 8,
        uv: UV | None = None,
        color: Color = (255, 255, 255),
        shades: bool = True,
        cap_start: bool = False,
        cap_end: bool = True,
        proxy: bool = True,
        ao: float = 1.0,
    ) -> None:
        """Tapered tube swept along a polyline (a tail, a hose, a curved pipe).

        Unlike :meth:`tube`, the path may curve and the radius may taper, and
        the rings are shared between neighbouring spans so there are no internal
        caps or doubled surfaces. Frames are parallel-transported along the path
        so the tube does not twist.
        """
        if len(points) < 2:
            raise ValueError("a swept tube needs at least two path points")
        path = [tuple(float(value) for value in entry) for entry in points]
        if isinstance(radii, (int, float)):
            radius_list = [float(radii)] * len(path)
        else:
            radius_list = [float(value) for value in radii]
            if len(radius_list) != len(path):
                raise ValueError("radii must match the number of path points")
        if any(radius <= 0.0 for radius in radius_list):
            raise ValueError("a swept tube needs positive radii")

        direction = _path_directions(path)
        frames: list[tuple[tuple[float, float, float], tuple[float, float, float]]] = []
        reference = (0.0, 1.0, 0.0) if abs(direction[0][1]) < 0.9 else (1.0, 0.0, 0.0)
        ux = _normalize(_cross(reference, direction[0]))
        uy = _normalize(_cross(direction[0], ux))
        frames.append((ux, uy))
        for index in range(1, len(path)):
            previous = frames[-1]
            projected = _cross(direction[index], _cross(previous[0], direction[index]))
            if _length(projected) < 1e-6:
                projected = previous[0]
            ux = _normalize(projected)
            uy = _normalize(_cross(direction[index], ux))
            frames.append((ux, uy))

        u0, v0, u1, v1 = uv or (0.0, 0.0, 1.0, 1.0)
        rings = [
            [_ring_point(path[index], frames[index][0], frames[index][1],
                         (step / segments) * math.tau, radius_list[index])
             for step in range(segments)]
            for index in range(len(path))
        ]
        for ring_index in range(len(path) - 1):
            lower, upper = rings[ring_index], rings[ring_index + 1]
            for index in range(segments):
                nxt = (index + 1) % segments
                angle_mid = ((index + 0.5) / segments) * math.tau
                multiplier = 1.0 if not shades else 0.74 + 0.26 * (0.5 + 0.5 * math.cos(angle_mid - 1.1))
                span = len(path) - 1
                self.quad(
                    lower[index],
                    lower[nxt],
                    upper[nxt],
                    upper[index],
                    [
                        (u0 + (u1 - u0) * (index / segments), v1 + (v0 - v1) * (ring_index / span)),
                        (u0 + (u1 - u0) * (nxt / segments), v1 + (v0 - v1) * (ring_index / span)),
                        (u0 + (u1 - u0) * (nxt / segments), v1 + (v0 - v1) * ((ring_index + 1) / span)),
                        (u0 + (u1 - u0) * (index / segments), v1 + (v0 - v1) * ((ring_index + 1) / span)),
                    ],
                    color,
                    shade_mult=multiplier,
                    ao=ao,
                )
        rect = (u0, v0, u1, v1)
        for index in range(segments):
            nxt = (index + 1) % segments
            if cap_start:
                self.triangle(
                    path[0],
                    rings[0][nxt],
                    rings[0][index],
                    _cap_uvs(rect),
                    color,
                    shade_mult=(1.0 if not shades else 0.7),
                    ao=ao,
                )
            if cap_end:
                self.triangle(
                    path[-1],
                    rings[-1][index],
                    rings[-1][nxt],
                    _cap_uvs(rect),
                    color,
                    shade_mult=(1.0 if not shades else 0.95),
                    ao=ao,
                )

        if proxy:
            for index in range(len(path) - 1):
                start, end = path[index], path[index + 1]
                self.parts.append(
                    {
                        "shape": "tube",
                        "start": [round(value, 4) for value in start],
                        "end": [round(value, 4) for value in end],
                        "radius": round((radius_list[index] + radius_list[index + 1]) * 0.5, 4),
                        "color": _hex(color),
                    }
                )

    def plane(self, center, size, uv: UV, color: Color, shade_mult: float = 1.0, normal: str = "y",
              rotation: Sequence[float] | None = None, proxy: bool = False, ao: float = 1.0) -> None:
        """Single outward-facing quad (rugs, screens, panels)."""
        cx, cy, cz = (float(value) for value in center)
        sx, sy, sz = (float(value) for value in size)
        hx, hy, hz = sx * 0.5, sy * 0.5, sz * 0.5
        if normal == "y":
            corners = [
                (cx - hx, cy, cz - hz),
                (cx + hx, cy, cz - hz),
                (cx + hx, cy, cz + hz),
                (cx - hx, cy, cz + hz),
            ]
        elif normal == "z":
            corners = [
                (cx - hx, cy - hy, cz),
                (cx + hx, cy - hy, cz),
                (cx + hx, cy + hy, cz),
                (cx - hx, cy + hy, cz),
            ]
        elif normal == "-z":
            corners = [
                (cx + hx, cy - hy, cz),
                (cx - hx, cy - hy, cz),
                (cx - hx, cy + hy, cz),
                (cx + hx, cy + hy, cz),
            ]
        elif normal == "x":
            corners = [
                (cx, cy - hy, cz + hz),
                (cx, cy - hy, cz - hz),
                (cx, cy + hy, cz - hz),
                (cx, cy + hy, cz + hz),
            ]
        else:  # "-x"
            corners = [
                (cx, cy - hy, cz - hz),
                (cx, cy - hy, cz + hz),
                (cx, cy + hy, cz + hz),
                (cx, cy + hy, cz - hz),
            ]
        if rotation:
            corners = [
                tuple(c + r for c, r in zip(_rotate(tuple(p[i] - c for i, c in enumerate((cx, cy, cz))), tuple(rotation)), (cx, cy, cz)))
                for p in corners
            ]
        self.quad(*corners, uv=uv, color=color, shade_mult=shade_mult, ao=ao)
        if proxy:
            self.parts.append(
                {
                    "shape": "plane",
                    "center": [round(cx, 4), round(cy, 4), round(cz, 4)],
                    "size": [round(sx, 4), round(sy, 4), round(sz, 4)],
                    "normal": normal,
                    "color": _hex(color),
                }
            )

    # -------------------------------------------------------------- internals

    def _add_vertex(self, point, color: Color, uv: tuple[float, float], shade_mult: float, ao: float) -> None:
        x, y, z = (float(value) for value in point)
        tint = shade_mult * ao
        rgb = shade(color, tint) if tint != 1.0 else color
        self.positions.append((x, y, z))
        self.colors.append((int(rgb[0]), int(rgb[1]), int(rgb[2])))
        self.uvs.append((float(uv[0]), float(uv[1])))

    # --------------------------------------------------------------- metrics

    @property
    def triangle_count(self) -> int:
        return len(self.indices) // 3

    @property
    def vertex_count(self) -> int:
        return len(self.positions)

    def translate(self, offset: Sequence[float]) -> None:
        """Shifts every vertex and every recorded proxy part by ``offset``."""
        dx, dy, dz = (float(value) for value in offset)
        if dx == 0.0 and dy == 0.0 and dz == 0.0:
            return
        self.positions = [
            (x + dx, y + dy, z + dz) for (x, y, z) in self.positions
        ]
        for part in self.parts:
            for key in ("center", "base", "start", "end"):
                if key in part:
                    part[key] = [
                        round(part[key][0] + dx, 4),
                        round(part[key][1] + dy, 4),
                        round(part[key][2] + dz, 4),
                    ]

    def normalize_origin(self) -> tuple[float, float, float]:
        """Puts the mesh on the pack's origin convention and reports the shift.

        Props must be horizontally centred with their base at ``y = 0``. A prop
        whose silhouette is deliberately asymmetric (a cat's tail reaches much
        further back than its nose reaches forward) builds in its natural
        coordinates and then calls this, so the placement origin is still the
        bounding-box centre that levels, the editor and collision all assume.
        """
        low, high = self.bounds()
        offset = (
            -round((low[0] + high[0]) * 0.5, 6),
            -round(low[1], 6),
            -round((low[2] + high[2]) * 0.5, 6),
        )
        self.translate(offset)
        return offset

    def bounds(self) -> tuple[tuple[float, float, float], tuple[float, float, float]]:
        if not self.positions:
            raise ValueError("mesh has no vertices")
        xs = [p[0] for p in self.positions]
        ys = [p[1] for p in self.positions]
        zs = [p[2] for p in self.positions]
        return (min(xs), min(ys), min(zs)), (max(xs), max(ys), max(zs))

    def dimensions(self) -> tuple[float, float, float]:
        low, high = self.bounds()
        return (high[0] - low[0], high[1] - low[1], high[2] - low[2])

    def validate(self, expected_size: Sequence[float], tolerance: float | None = None) -> None:
        """Raises ``ValueError`` with an actionable message when the mesh breaks the pack rules."""
        if self.triangle_count == 0:
            raise ValueError("mesh has no triangles")
        if self.vertex_count > 65535:
            raise ValueError(f"mesh has {self.vertex_count} vertices; 16-bit indices allow 65535")
        for position in self.positions:
            for value in position:
                if not math.isfinite(value):
                    raise ValueError("mesh contains a non-finite vertex coordinate")
        for u, v in self.uvs:
            if not math.isfinite(u) or not math.isfinite(v):
                raise ValueError("mesh contains a non-finite UV")
            if u < -0.001 or u > 1.001 or v < -0.001 or v > 1.001:
                raise ValueError(f"UV {u:.3f},{v:.3f} is outside 0..1; the pack uses non-tiling UVs")
        for index, value in enumerate(self.indices):
            if value < 0 or value >= self.vertex_count:
                raise ValueError(f"index {index} points at vertex {value} outside the mesh")
        low, high = self.bounds()
        for axis, name in enumerate("xyz"):
            if high[axis] - low[axis] <= 1e-4:
                raise ValueError(f"mesh has zero extent on the {name} axis")
        if abs(low[1]) > 0.012:
            raise ValueError(
                f"mesh base sits at y={low[1]:.3f}; props must rest on y=0 (origin at the floor contact point)"
            )
        center_x = (low[0] + high[0]) * 0.5
        center_z = (low[2] + high[2]) * 0.5
        if abs(center_x) > 0.02 or abs(center_z) > 0.02:
            raise ValueError(
                f"mesh is not horizontally centred (centre x={center_x:.3f}, z={center_z:.3f}); "
                "the origin must be under the object's centre"
            )
        dimensions = self.dimensions()
        for axis, name in enumerate("xyz"):
            allowed = tolerance if tolerance is not None else max(0.02, 0.06 * expected_size[axis])
            if abs(dimensions[axis] - expected_size[axis]) > allowed:
                raise ValueError(
                    f"mesh {name} extent {dimensions[axis]:.3f} m does not match the catalogue size "
                    f"{expected_size[axis]:.3f} m (tolerance {allowed:.3f} m)"
                )

    def degenerate_triangles(self) -> int:
        count = 0
        for index in range(0, len(self.indices), 3):
            a = self.positions[self.indices[index]]
            b = self.positions[self.indices[index + 1]]
            c = self.positions[self.indices[index + 2]]
            ab = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
            ac = (c[0] - a[0], c[1] - a[1], c[2] - a[2])
            cross = _cross(ab, ac)
            if _length(cross) < 1e-7:
                count += 1
        return count


class PropBuilder:
    """Bundle of the mesh, texture and metadata for a single prop.

    ``size`` is always the catalogue ``size`` (the generator reads
    ``assets/props/props.json``), so a prop cannot silently drift from the
    registry that levels and the editor resolve against.
    """

    def __init__(self, prop_id: str, name: str, size: Sequence[float], tex_size: int = 64, seed: int = 1,
                 ao_strength: float = 0.10, ao_height: float = 0.30) -> None:
        self.id = prop_id
        self.name = name
        self.size = tuple(float(value) for value in size)
        self.mesh = Mesh()
        self.tex = Texture(tex_size, seed=seed)
        # Subtle contact darkening near the floor: cheap, baked, and it keeps
        # props from looking like they float above the carpet.
        self.ao_strength = ao_strength
        self.ao_height = ao_height
        self.notes: list[str] = []

    # Convenience passthroughs so prop modules read naturally.
    @property
    def width(self) -> float:
        return self.size[0]

    @property
    def height(self) -> float:
        return self.size[1]

    @property
    def depth(self) -> float:
        return self.size[2]

    def box(self, center, size, **kwargs) -> None:
        kwargs.setdefault("color", (200, 200, 200))
        self.mesh.box(center, size, ao=self._ao(center, size), **kwargs)

    def cylinder(self, base, radius, height, **kwargs) -> None:
        kwargs.setdefault("color", (200, 200, 200))
        self.mesh.cylinder(base, radius, height, ao=self._ao_cylinder(base, height), **kwargs)

    def tube(self, start, end, radius, **kwargs) -> None:
        kwargs.setdefault("color", (200, 200, 200))
        midpoint = tuple((a + b) * 0.5 for a, b in zip(start, end))
        self.mesh.tube(start, end, radius, ao=self._ao(midpoint, (radius * 2,) * 3), **kwargs)

    def plane(self, center, size, **kwargs) -> None:
        kwargs.setdefault("color", (200, 200, 200))
        # Flat decals (rugs) must not be darkened by contact shading.
        self.mesh.plane(center, size, ao=self._ao(center, size), **kwargs)

    def lathe(self, base, profile, **kwargs) -> None:
        """Surface of revolution with floor-contact shading (see `Mesh.lathe`)."""
        kwargs.setdefault("color", (200, 200, 200))
        self.mesh.lathe(base, profile, ao=self._ao_at(base[1]), **kwargs)

    def tube_path(self, points, **kwargs) -> None:
        """Curved swept tube with floor-contact shading (see `Mesh.tube_path`)."""
        kwargs.setdefault("color", (200, 200, 200))
        lowest = min(point[1] for point in points)
        self.mesh.tube_path(points, ao=self._ao_at(lowest), **kwargs)

    def _ao(self, center, size) -> float:
        """Contact darkening factor for a primitive whose base sits at ``center[1] - size[1]/2``."""
        return self._ao_at(center[1] - size[1] * 0.5)

    def _ao_cylinder(self, base, height) -> float:
        del height
        return self._ao_at(base[1])

    def _ao_at(self, base_y: float) -> float:
        if self.ao_strength <= 0.0:
            return 1.0
        closeness = max(0.0, min(1.0, 1.0 - max(0.0, base_y) / max(1e-4, self.ao_height)))
        return 1.0 - self.ao_strength * closeness

    def add_note(self, text: str) -> None:
        self.notes.append(text)

    def set_texture(self, size: int, seed: int | None = None) -> Texture:
        """Replaces the canvas before any painting (128 is the pack maximum)."""
        self.tex = Texture(size, seed=seed if seed is not None else _stable_seed(self.id))
        return self.tex


def _cap_uvs(rect: UV) -> list[tuple[float, float]]:
    u0, v0, u1, v1 = rect
    mid_u = (u0 + u1) * 0.5
    mid_v = (v0 + v1) * 0.5
    return [(mid_u, mid_v), (u0, v1), (u1, v1)]


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _length(v) -> float:
    return math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2])


def _normalize(v):
    length = _length(v)
    if length < 1e-9:
        return (0.0, 0.0, 1.0)
    return (v[0] / length, v[1] / length, v[2] / length)


def _ring_point(center, ux, uy, angle: float, radius: float):
    cos, sin = math.cos(angle), math.sin(angle)
    return (
        center[0] + (ux[0] * cos + uy[0] * sin) * radius,
        center[1] + (ux[1] * cos + uy[1] * sin) * radius,
        center[2] + (ux[2] * cos + uy[2] * sin) * radius,
    )


def _path_directions(path):
    """Unit tangent at every node of a polyline (central difference in the middle)."""
    directions = []
    for index in range(len(path)):
        if index == 0:
            delta = _subtract(path[1], path[0])
        elif index == len(path) - 1:
            delta = _subtract(path[-1], path[-2])
        else:
            delta = _subtract(path[index + 1], path[index - 1])
        directions.append(_normalize(delta))
    return directions


def _subtract(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _hex(color: Color) -> str:
    return "#%02x%02x%02x" % (int(color[0]), int(color[1]), int(color[2]))


def _stable_seed(text: str) -> int:
    """Deterministic seed from a prop id (Python's hash() is salted per process)."""
    value = 0
    for index, char in enumerate(text):
        value = (value * 131 + ord(char) + index) % 100000
    return value + 1
