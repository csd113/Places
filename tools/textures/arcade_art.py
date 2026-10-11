#!/usr/bin/env python3
"""Arcade texture registry; normal exports preserve committed PNG artwork.

Deliberate import and source-to-native conversion live in author_arcade.py.
Neither the normal build nor the runtime paints these sheets.
"""

from home_art import load_sheet

CARPETS = (
    "cosmic_bowl", "neon_confetti", "starburst", "retro_swirl",
    "geometric", "arcade_grid",
)

SHEETS = {}
for family in CARPETS:
    for suffix in ("01", "uv_mask_01"):
        name = f"carpet_{family}_{suffix}"
        SHEETS[f"arcade:tex_{name}"] = f"environments/arcade/textures/floors/{name}.png"
for directory, name in (
    ("walls", "wall_navy_01"),
    ("ceilings", "ceiling_acoustic_01"),
    ("floors", "lane_wood_01"),
    ("floors", "gutter_dark_01"),
):
    SHEETS[f"arcade:tex_{name}"] = f"environments/arcade/textures/{directory}/{name}.png"
SHEETS["arcade:fluorescent_panel_01"] = "environments/arcade/textures/lights/fluorescent_panel_01.png"
SHEETS["arcade:blacklight_tube"] = "environments/arcade/textures/lights/blacklight_tube_01.png"
for name in ("bowling_mural_01", "lane_arrow_01", "foul_line_01"):
    SHEETS[f"arcade:decal_{name}"] = f"environments/arcade/decals/{name}.png"

ART = {
    asset_id: {"model": model, "build": lambda model=model: load_sheet(model)}
    for asset_id, model in SHEETS.items()
}
