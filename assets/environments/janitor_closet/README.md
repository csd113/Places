# Janitor / Supplies

15 accepted static GLBs and their exact standalone PNGs, registered under the
`janitor:` namespace and `janitor_closet` theme. The blue folded-towel preset
shares the ivory towel's GLB, UVs and PNG; its linear RGB multiplier is
`[0.11, 0.19, 0.30]`. No runtime reads the delivery's material-choices JSON.

Models use metres, +Y up and +Z front with the delivered base pivot. Catalogue
sizes conservatively cover asymmetric handles about that pivot; the exact
occupied min/max, hashes, triangles and texture edges are in
`assets/completed-models.json`. Runtime image semantics are opaque and clamped.
The fitted 1024² material atlas and 256×384 caution print retain their delivered
pixels. Do not repack UVs or substitute procedural textures.

Reusable physics lives in `assets/gameplay.json`. Rack and sink use fixed open
compounds. The rack's four shelves have top contact at .1625, .6625, 1.1625 and
1.6625 m. Its posts remain collidable while the shelf gaps stay open to items.
The sink's basin walls/bottom and economical stand leave the bowl accessible;
its visible faucet is above the .89 m standable rim. Fixed player collision is
separate: author a placement `size` and `solid` deliberately (sink height .89).

Portable supplies use dynamic pickup profiles with conservative cuboids or
cylinders. The mop combines its head and narrow handle; the crate has an open
compound, and the wet-floor sign uses a triangular-prism hull. Cart and wringer
bucket are movable without pickup and use economical open compounds. These
are contact approximations, not triangle-mesh collision, articulated wheels or
a simulated cloth/liquid system. Newly authored dynamic placements start dormant.
Copy the intended profile into a level placement explicitly; a catalogue profile
only supplies editor/developer spawn defaults.

`projects/janitor-supplies/` is the editable, opt-in closet preview. It includes
all 15 models, a shared-mesh towel colour choice, shelf loads, real item pickup,
and an ordinary switch for the light, switch animation and item reset. Existing
game modes and showcase placements are not changed by this environment.

Complete editable Blender/GIMP sources, reference, three-pass checklists and
fresh-GLB previews are preserved in the final Downloads delivery. The integration
report records their original Library IDs and the installed hashes separately.
