# Frutiger Aero kit

The immutable `../Frutiger Aero Places Concept Sheet.png` is the reference.
The finished connected level is `assets/levels/frutiger_aero_demo.json`, authored
by `tools/levels/build_frutiger_aero.py`; the player loads its sibling package.

| Reference | Catalog models / artwork |
| --- | --- |
| A01 / 01 | `wall_bay`, `wall_bay_tall`: rounded segmented white frame, cyan glazing, returning end, leaf |
| A02 / 02 | `lime_arch`: faceted round crown, straight piers, lime reveal, cyan base |
| A03 / 03 | `glass_partition`: exactly three folded leaf-marked cyan panels |
| A04 / 04 | `light_pod`: octagonal recessed cyan ring, three 3.6 m suspension lines |
| A05 / 05 | `tree_planter`: rounded aqua-striped pot, branched brown trunk, angular canopy and basal leaves |
| A06 / 06 | `seating_pod`: curved white shell, two cyan seats/backs, descending arms and short feet |
| A07 / 07 | `display_kiosk`: cyan display, lime inset, white plinth and A BRIGHTER TOMORROW |
| A08 / 08 | `fountain_basin`, `bubble_sculpture`: open circular basin and three translucent faceted bubbles with leaves |
| A09 / 09 | `glass_canopy`: true open barrel-vault cyan tube and white end frames |
| A10 / 10 | `accent_panel`: diagonal white stripe and lime leaves on tall cyan panel |
| A11 | `atrium_dome`: 12 m glazed roof, 16 connected ribs and circular soffit |
| A12 | `ceiling_ring`, `double_doors`, `double_doors_open`: corridor ring and gray terminal doors |
| A13 | `reception_counter`, `reception_soffit`: curved aqua-based counter, two terminals, rounded canopy and warm downlights |
| A14 | `banner_atrium`, `banner_corridor`, `banner_reception`, five fitted decal sheets |
| A15 | `city_backdrop`, `green_backdrop`, `tex_sky_day_01` |

All model IDs use the `frutiger_aero:` prefix. Geometry is deliberately faceted,
in metres, +Y up and +Z front, with centered horizontal bounds and a floor
contact origin. The 23 models total 6,864 triangles. The 1,152-triangle dome
retains meaningful structural detail within the existing 1,500-triangle asset
ceiling; other modules remain below the 800-triangle review threshold.

The six surface IDs are `white_panel_01`, `white_floor_01`, `white_ceiling_01`,
`aqua_tile_01`, `cyan_glass_01` and `cyan_water_01`, all with the same prefix.
Reusable decal IDs are `decal_atrium_banner_01`, `decal_corridor_typography_01`,
`decal_reception_banner_01`, `decal_kiosk_typography_01` and `decal_leaf_brand_01`.
There are no new animation clips or character assets.

`tools/props/parts/frutiger_aero.py` loads the committed atlas and exports the
models. `structural_components` / `placed_components` provide tight collision
stock for bays, faceted arches, folded panels, seats, planters, doors, tube and
counter. Hollow visuals remain `solid:false`; never collide their whole bounds.
The canopy uses facet-aligned roof stock and 18 µm carrier meshes: its explicit
physical extents remain full size while opaque carrier pixels stay subpixel
through BLEND glazing. Opaque structural stock uses enclosed 1.8 mm carriers.
Map floors remain separate. Basin support uses collider strips inside its real
floor slab, with their top 5 mm below the visible surface; actual circular water
is radius 1.30 m, surface Y .37, bottom .11, and `swimming:false`.

The committed 256² atlas is a derivative of its retained 1024² master. Its
top 64 pixels contain sixteen 32² palette cells; rows 64–192 contain four 64×128
branding panels; the bottom 64 pixels contain accent, leaf, ring and terminal
art. Preserve those UV regions. Five dedicated 256² atlases retain 1024² masters
for the accent, kiosk and three banners. Their fitted 1:2 faces occupy the left
128×256 pixels (512×1024 in the master), with white/lime/cyan/silver stock
swatches on the right. They remain opaque, top-down, +Z upright and UV-clamped.
Scalar BLEND opacity .17/.16/.13 controls
panel/tube/dome glazing; bubbles use .55. PNG atlas alpha stays 255.

Four 512² seamless surface PNGs retain 1024² masters: white panel, aqua tile,
cyan glass and cyan water. Glass carries straight alpha 52–64. Five fitted POT
decals carry transparent margins: atrium, reception and kiosk are 512×1024;
corridor and leaf are 512². Their placements retain those proportions. The
2048×1024 equirectangular panorama repeats U and places its sun at +45° bearing
and elevation; the authored directional light matches it.

`tools/textures/frutiger_aero_art.py --author` is an explicit offline artwork
export; `--check` reproduces all 26 PNGs without writing. Normal texture and
model builds load real repository PNGs. No runtime painting is used.

Use existing lighting, water, alpha sorting and scalar sheen facilities. Glass
does not refract; model materials do not gain probe reflections or normal maps.
No creature or locomotion animation is depicted by the sheet or required by
this kit. The open doors are a static accessible variant.

Validation and genuine native evidence are recorded in the single program
report: `docs/two-environment-implementation-plan.md`.
