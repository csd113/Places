"""Reusable birthday arrangements expressed as ordinary revision 1 placed props.

The caller provides catalog sizes and a real room/supporting floor. All four
tables can be placed in any map at a new centre and yaw. Balloons are individual
instances of the shared typed entity; groups own neither a combined mesh nor
physics. Merge both props and timers into the level; the cake's one initialization
timer starts its authored flame clip. This never bakes lighting or writes a level.
"""
from __future__ import annotations

import math
from collections.abc import Sequence

GIFT_DESIGNS = ("stars", "confetti", "stripes", "arcade", "red", "green")
BALLOON_COLORS = ("red", "royal_blue", "golden_yellow", "grass_green",
                  "deep_purple", "orange", "pink")
TABLE_TOP = .753
GIFT_SUPPORT = .302


class PartyComposer:
    """Append reusable, deterministic arrangements to a caller-owned prop list."""

    def __init__(self, sizes: dict[str, list[float]], *, weight_anchor_height=.05442):
        self.sizes = sizes
        self.weight_anchor_height = weight_anchor_height
        self.props: list[dict] = []
        self.timers: list[dict] = []

    def prop(self, name, x, z, identity, *, y=0, yaw=0, solid=False, **extra):
        model = name if ":" in name else "arcade:party_" + name
        entry = dict(id=identity, model=model, x=round(x, 6), y=round(y, 6),
                     z=round(z, 6), rotation_degrees=round(yaw % 360, 6),
                     size=self.sizes[model], solid=solid, occludes=False, **extra)
        self.props.append(entry)
        return entry

    def balloon_bouquet(self, x, z, identity, *, count=3, y=0, yaw=0,
                        colors: Sequence[str] = BALLOON_COLORS,
                        string_length=1.35, shared_phase=.17, phase_offset=.13):
        """Place 1..16 independently moving latex balloons at one fixed tie.

        Three/five/seven are useful presets. Other counts use the same entity
        and a ring radius that maintains clearance at the maximum sway. All
        strings start at the exact same attachment coordinate; only their upper
        endpoints fan out. The caller keeps the swept envelope inside its room.
        """
        if isinstance(count, bool) or not isinstance(count, int) or not 1 <= count <= 16:
            raise ValueError("a bouquet needs between one and sixteen balloons")
        if not colors or any(color not in BALLOON_COLORS for color in colors):
            raise ValueError("bouquet colours must be supported latex colours")
        if count == 1:
            offsets = [(.05, .08)]
        elif count == 3:
            offsets = [(-.38, .08), (.38, .03), (0, -.36)]
        elif count == 7:
            offsets = [(0, 0)] + [(.56 * math.cos(index * math.tau / 6),
                                   .56 * math.sin(index * math.tau / 6))
                                  for index in range(6)]
        else:
            radius = max(.50, count * .44 / math.tau)
            offsets = [(radius * math.cos(index * math.tau / count),
                        radius * math.sin(index * math.tau / count))
                       for index in range(count)]
        for index, (dx, dz) in enumerate(offsets):
            length = string_length + (index * 2 % 5) * .075
            # The central seventh balloon rises above its six neighbours.
            if count == 7 and index == 0:
                length += .30
            self.prop("balloon", x, z, f"{identity}_{index}", y=y, yaw=yaw,
                components=[dict(component="balloon", color=colors[index % len(colors)],
                    scale=.94 + (index % 3) * .025, string_length=round(length, 6),
                    anchor=[0, 0, 0], offset=[round(dx, 6), 0, round(dz, 6)],
                    enabled=True, sway_amplitude=.022 + (index % 3) * .003,
                    bob_amplitude=.010 + (index % 2) * .003,
                    rotation_degrees=2.7 + (index % 2) * .4,
                    speed_hz=.16 + (index % 4) * .018,
                    phase=round((phase_offset + index * .217) % 1, 6),
                    shared_phase=shared_phase, shared_sway=.007)])

    def gift_group(self, x, z, identity, *, count=5, y=0, yaw=0,
                   designs: Sequence[str] = GIFT_DESIGNS):
        """Equal .30 m cubes: a stable bridge stack of three, five or seven+.

        The shared bow is off centre. Lower rotations send those bows away
        from the central upper cube; the upper cube rests across inward top
        corners, not on the bows. Seven adds two separate foundation gifts.
        More gifts extend the foundation, with no scaled or flattened boxes.
        """
        if count not in (3, 5) and not 7 <= count <= 16:
            raise ValueError("gift groups support three, five or seven to sixteen gifts")
        if not designs or any(design not in GIFT_DESIGNS for design in designs):
            raise ValueError("gift designs must be registered wrapping variants")
        placement = _Placement(self, x, z, yaw, identity)
        if count == 3:
            positions = [(-.17, 0, 0, 135), (.17, 0, 0, 315),
                         (0, 0, GIFT_SUPPORT, -8)]
        else:
            positions = [(.195, .195, 0, 270), (.195, -.195, 0, 0),
                         (-.195, -.195, 0, 90), (-.195, .195, 0, 180),
                         (0, 0, GIFT_SUPPORT, -3)]
            for index in range(count - 5):
                side = -1 if index % 2 == 0 else 1
                row = index // 2
                positions.append((side * .75, row * .39, 0,
                                  12 if side < 0 else -14))
        for index, (dx, dz, height, rotation) in enumerate(positions):
            placement.prop("gift_" + designs[index % len(designs)], dx, dz,
                           str(index), y=y + height, yaw=rotation, scale=1.0)

    def simple_rect(self, x, z, identity, *, yaw=0):
        placement = _Placement(self, x, z, yaw, identity)
        placement.table("rect", "confetti")
        placement.rect_chairs()
        # Six place settings, with four hats resting on their paper plates.
        for index, (dx, dz, pattern, hat) in enumerate((
            (-.56, -.25, "confetti", "dots"), (.56, -.25, "balloons", "stars"),
            (-.56, .25, "stars", "confetti"), (.56, .25, "border", "shapes"),
            (-.86, 0, "confetti", None), (.86, 0, "balloons", None),
        )):
            placement.prop("plate_"+pattern, dx, dz, f"plate_{index}", y=TABLE_TOP)
            if hat:
                placement.prop("hat_"+hat, dx, dz, f"hat_{index}",
                               y=TABLE_TOP+.006, yaw=11+index*29)
        for index, (dx, dz, pattern) in enumerate((
            (-.28, -.24, "confetti"), (.28, -.24, "balloons"),
            (-.28, .24, "stars"), (.28, .24, "geometric"),
            (-.86, -.26, "confetti"), (.86, .26, "balloons"),
        )):
            placement.prop("cup_"+pattern, dx, dz, f"cup_{index}",
                           y=TABLE_TOP, yaw=7+index*23)
        # Tie each single to the actual top of an end chair's thick back.
        placement.bouquet(-1.70984, .00907, "single_red", count=1, y=.91,
                          colors=("red",), string_length=1.38, shared_phase=.08, phase_offset=.11)
        placement.bouquet(1.70996, .00454, "single_pink", count=1, y=.91,
                          colors=("pink",), string_length=1.49, shared_phase=.54, phase_offset=.58)

    def present_table(self, x, z, identity, *, yaw=0):
        placement = _Placement(self, x, z, yaw, identity)
        placement.table("rect", "arcade")
        placement.gifts(0, 0, "stack", count=7, y=TABLE_TOP)
        placement.prop("cup_geometric", -.49, -.27, "cup", y=TABLE_TOP, yaw=24)
        placement.prop("plate_border", .47, -.31, "plate", y=TABLE_TOP, yaw=9)
        placement.weighted_bouquet(-.75, -.32, "bouquet", count=7,
                                  string_length=1.30, shared_phase=.32, phase_offset=.20)

    def round_table(self, x, z, identity, *, yaw=0):
        placement = _Placement(self, x, z, yaw, identity)
        placement.table("round", "balloons")
        for index, (dx, dz, rotation, color) in enumerate((
            (-1.03, 0, 87, "red"), (1.03, 0, 272, "blue"),
            (0, -1.03, 2, "blue"), (0, 1.03, 179, "red"),
        )):
            placement.prop("arcade:fixed_chair_"+color, dx, dz, f"chair_{index}",
                           yaw=rotation, solid=True)
        for index, (dx, dz, pattern, hat) in enumerate((
            (-.28, -.23, "balloons", "stripes"), (.28, -.23, "stars", None),
            (-.28, .23, "confetti", None), (.28, .23, "border", "dots"),
        )):
            placement.prop("plate_"+pattern, dx, dz, f"plate_{index}", y=TABLE_TOP)
            if hat:
                placement.prop("hat_"+hat, dx, dz, f"hat_{index}",
                               y=TABLE_TOP+.006, yaw=index*38)
        for index, (dx, dz, pattern) in enumerate((
            (-.46, .03, "balloons"), (.46, .03, "stars"),
            (-.05, -.46, "confetti"), (-.05, .46, "geometric"),
        )):
            placement.prop("cup_"+pattern, dx, dz, f"cup_{index}", y=TABLE_TOP, yaw=index*47)
        placement.weighted_bouquet(0, 0, "bouquet", count=3,
            colors=("royal_blue", "golden_yellow", "grass_green"),
            string_length=1.25, shared_phase=.71, phase_offset=.43)

    def cake_table(self, x, z, identity, *, yaw=0, lit=True):
        placement = _Placement(self, x, z, yaw, identity)
        placement.table("rect", "stars")
        # A clear approach edge gives the small cake the whole visual centre.
        # Two far-side chairs remain, with no chair across the near sightline.
        for index, (dx, rotation, color) in enumerate((
            (-.56, 174, "blue"), (.56, 184, "red"),
        )):
            placement.prop("arcade:fixed_chair_"+color, dx, .94, f"chair_{index}",
                           yaw=rotation, solid=True)
        components = []
        if lit:
            components = [dict(component="animation", clip="candle_flicker", looped=True, playing=False),
                          dict(component="glow", color=[1, .49, .16], intensity=.28,
                               range=.80, offset=[0, .218, 0], fade=False, enabled=True)]
        cake = placement.prop("cake_lit" if lit else "cake_unlit", -.28, 0, "cake",
                              y=TABLE_TOP, components=components)
        if lit:
            self.timers.append(dict(id=identity+"_candles_start", seconds=.01,
                autostart=True, repeat=False, bindings=[dict(on="timer", actions=[
                    dict(action="play_animation", target=cake["id"],
                         clip="candle_flicker", loop=True)])]))
        placement.gifts(.67, 0, "gifts", count=3, y=TABLE_TOP,
                        designs=("green", "red", "confetti"))
        for index, (dx, dz, pattern) in enumerate((
            (-.76, -.23, "stars"), (.12, .25, "confetti"),
        )):
            placement.prop("plate_"+pattern, dx, dz, f"plate_{index}", y=TABLE_TOP)
        placement.prop("hat_stripes", -.76, -.23, "hat", y=TABLE_TOP+.006, yaw=17)
        placement.prop("cup_stars", -.68, .23, "cup", y=TABLE_TOP, yaw=61)
        placement.prop("cup_balloons", .12, -.26, "cup_2", y=TABLE_TOP, yaw=9)
        placement.weighted_bouquet(-.92, .10, "bouquet", count=5,
            colors=("deep_purple", "pink", "golden_yellow", "royal_blue", "orange"),
            string_length=1.32, shared_phase=.44, phase_offset=.67)


class _Placement:
    """One arrangement's local metres transformed by the normal prop yaw."""

    def __init__(self, composer, x, z, yaw, identity):
        if not all(math.isfinite(value) for value in (x, z, yaw)):
            raise ValueError("party placement coordinates and yaw must be finite")
        self.composer, self.x, self.z = composer, x, z
        self.yaw, self.identity = yaw, identity
        self.cos = math.cos(math.radians(yaw))
        self.sin = math.sin(math.radians(yaw))

    def point(self, x, z):
        return self.x + self.cos*x + self.sin*z, self.z - self.sin*x + self.cos*z

    def prop(self, name, x, z, key, *, yaw=0, **extra):
        px, pz = self.point(x, z)
        return self.composer.prop(name, px, pz, self.identity+"_"+key,
                                  yaw=self.yaw+yaw, **extra)

    def table(self, kind, cloth):
        self.prop("table_"+kind, 0, 0, "table", solid=True)
        self.prop("cloth_"+kind+"_"+cloth, 0, 0, "cloth", y=.502 if kind == "rect" else .52282)

    def rect_chairs(self):
        for index, (dx, dz, rotation, color) in enumerate((
            (-.56, -.94, 3, "red"), (.56, -.94, -3, "blue"),
            (-.56, .94, 182, "blue"), (.56, .94, 176, "red"),
            (-1.45, 0, 92, "red"), (1.45, 0, 269, "blue"),
        )):
            self.prop("arcade:fixed_chair_"+color, dx, dz, f"chair_{index}",
                      yaw=rotation, solid=True)

    def bouquet(self, x, z, key, **extra):
        px, pz = self.point(x, z)
        self.composer.balloon_bouquet(px, pz, self.identity+"_"+key,
                                     yaw=self.yaw, **extra)

    def weighted_bouquet(self, x, z, key, **extra):
        """Tie every string above the cloth to the same real molded loop."""
        self.prop("balloon_weight", x, z, key+"_weight", y=TABLE_TOP)
        self.bouquet(x, z, key, y=TABLE_TOP+self.composer.weight_anchor_height, **extra)

    def gifts(self, x, z, key, **extra):
        px, pz = self.point(x, z)
        self.composer.gift_group(px, pz, self.identity+"_"+key,
                                 yaw=self.yaw, **extra)
