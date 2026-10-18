# Janitor / Supplies

Open this folder with the direct `places-editor` executable. This small opt-in
preview is separate from the shipped level list and both completed game modes.
It presents all 15 new models, shelf and cart loads, portable item physics and
one blue towel instance using the existing shared-mesh tint preset.

The entry level is a static editable layout so the inspect viewport displays
every model. Select `janitor_physics` for explicit profiles: E picks up portable
supplies, and E on the wall switch toggles the real light and its rebuilt
`toggle` animation and resets the `closet_supplies` physics group. The static
layout switch controls light and animation only.
F supplies a flashlight for material inspection in darkness. The rack/sink have
open compounds for rigid items; their authored player boxes are conservative.
The sink's player height is the .89 m rim, below its decorative faucet.

Build/export explicitly with `places-compile export-project`; this project is
never a required test or CI bake. Runtime assets live under
`assets/janitor_closet/`, profiles under `assets/gameplay.json`, and editable
Blender/GIMP source/checklists are preserved in the Downloads integration folder.
