"""Shared colour palette for the places core prop pack.

Every prop in the pack draws its colours from this module so the twenty
finished assets read as one coherent, faded, institutional/domestic set at
480x272.  Keep additions muted: no neon, no saturated showroom plastics.

Colours are stored as ``#rrggbb`` strings (the same spelling the prop
catalogue uses) and converted to 0..255 tuples on demand.
"""

from __future__ import annotations

from typing import Iterable

# --------------------------------------------------------------------- base

# Faded painted metal (cabinets, appliances, industrial furniture).
METAL_GREY = "#9aa0a3"
METAL_LIGHT = "#b4b8ba"
METAL_DARK = "#5f6468"
METAL_SHADOW = "#41464a"
CHROME = "#c2c6c8"

# Yellowed / dulled plastics.
PLASTIC_WHITE = "#e2ddd0"
PLASTIC_CREAM = "#d8cfb8"
PLASTIC_BEIGE = "#c9bfa6"
PLASTIC_GREY = "#b9b7ae"
PLASTIC_DARK = "#6d6a63"

# Wood.
WOOD_WARM = "#8a6f4d"
WOOD_MID = "#7a6244"
WOOD_DARK = "#5c4830"
WOOD_VENEER = "#8d7351"
WOOD_PALE = "#a08a68"

# Upholstery / fabric.
FABRIC_BEIGE = "#a99a80"
FABRIC_TAN = "#b7a488"
FABRIC_BROWN = "#8a7458"
FABRIC_OLIVE = "#7e7a5c"
FABRIC_GREY = "#8d8b85"
FABRIC_BLUE = "#63758a"
FABRIC_RED = "#7c4f45"
MATTRESS_WHITE = "#cfc9bb"
SHEET_GREY = "#b9b5ab"

# Institutional paint accents (used sparingly).
INSTITUTIONAL_GREEN = "#6f7f6a"
INSTITUTIONAL_BLUE = "#5a6b7c"
INSTITUTIONAL_TEAL = "#5f7a78"
WALL_CREAM = "#d9d2bd"

# Electronics and glass.
ELECTRONICS_DARK = "#33363a"
ELECTRONICS_BLACK = "#25282c"
SCREEN_DARK = "#20242a"
SCREEN_GLASS = "#2b3138"
GLASS_TINT = "#8fa2ab"
BOTTLE_BLUE = "#a8c4cd"

# Utility.
CARDBOARD = "#a8895f"
CARDBOARD_DARK = "#8b6f47"
CARDBOARD_TAPE = "#c3b48c"
CRATE_WOOD = "#8b6f47"
CRATE_DARK = "#6b5233"
PAPER = "#cfc6ae"

# Nature.
FOLIAGE_GREEN = "#4f6b43"
FOLIAGE_DARK = "#3f5735"
FOLIAGE_LIGHT = "#61804f"
STEM_GREEN = "#5d6b3f"

# Wear / age layers.
GRIME = "#4a4238"
RUST = "#7a5a44"
RUG_BROWN = "#7d6a5e"
RUG_TEAL = "#5c6b68"
RUG_RED = "#7a4f45"


def hex_to_rgb(value: str) -> tuple[int, int, int]:
    """Converts ``#rrggbb`` (or ``rrggbb``) to an ``(r, g, b)`` byte tuple."""
    text = value.strip().lstrip("#")
    if len(text) != 6:
        raise ValueError(f"expected a #rrggbb colour, got {value!r}")
    return (int(text[0:2], 16), int(text[2:4], 16), int(text[4:6], 16))


def rgb_to_unit(rgb: Iterable[int]) -> tuple[float, float, float]:
    """Converts a byte tuple to 0..1 floats (the form written to vertex colours)."""
    r, g, b = rgb
    return (r / 255.0, g / 255.0, b / 255.0)


def shade(rgb: tuple[int, int, int], mult: float) -> tuple[int, int, int]:
    """Multiplies a colour by ``mult``, clamped to 0..255."""
    return tuple(max(0, min(255, int(round(channel * mult)))) for channel in rgb)  # type: ignore[return-value]


def mix(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    """Linear blend from ``a`` to ``b`` (``t`` clamped to 0..1)."""
    t = max(0.0, min(1.0, t))
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))  # type: ignore[return-value]
