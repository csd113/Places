# NPC texture refresh — November 2026

The twelve shipped NPC/creature skins now use material artwork in their existing fitted UV frames: charcoal/ivory fur for Spooner-Man, grey composite for the mannequin, brown-grey fur and dusty rose skin for the rat, aged ivory bone for both skeleton variants, orange rind and woody stems for the pumpkins, woven cloth for the ghosts, chestnut chitin for the cockroach, and shell/scales/feathers for the Beach animals. The existing model silhouettes and colour identities remain.

`assets/entities/<entity>/textures/surface_01.png` is the current standalone runtime artwork. Models embed those exact PNGs. The Beach `surface_master.png` sheets retain 1024-square sources; their 256-square derivatives use the existing exact 4×4 box filter. Other sheets retain their previous dimensions except the cockroach's uniform 32→128-square increase (60 KiB additional decoded RGBA). The two skeleton GLBs embed the same bone PNG.

Only image bytes, GLB buffer offsets/lengths, approved export hashes and the ghost's exact-byte pin change. Every non-image bufferView and every other JSON value is preserved, including indices, positions, UVs, normals, vertex colours, joints/weights, inverse binds, transforms, clips, material assignments, emission, alpha and collision metadata. Rat eye islands, fish stripe silhouettes, recessed dark swatches and pumpkin glow swatches retain their fitted positions. The black-mannequin preset continues to share the same rig/texture and unchanged tint.

The built-in image-generation tool produced two material source sheets. Offline cropping/resampling places their material artwork inside the existing atlas rectangles; it does not create runtime imagery or modify UVs. Source PNGs are authoritative; historical constructors must not restore the older completed exports. Old named/delivered PNGs remain historical sources. Rebuild the player and compiler after changing `completed-models.json`, because its identities are compiled into them.

## Visual and structural acceptance

Native before/after screenshots use a small temporary gallery with identical cameras, High texture quality/filtering, Static lighting, fixed 1/60-second simulation steps, frame 25, bloom/reflections off and a 1600×1200 physical drawable on macOS Metal. The executable is launched directly, with isolated state. This is an appearance check, not a performance comparison. Curated [native comparisons](../images/npc-textures-2026-11-03/native-before-after.png) and the [all-NPC comparison](../images/npc-textures-2026-11-03/all-npcs.png) record the actual game renderer.

The source/export preservation check covers all thirteen GLBs, 4,649 unchanged non-image bufferViews and PNG parity. The fourteen affected editor catalog previews (including both aliases) are refreshed through the existing preview renderer, with current model hashes. Catalog validation, the accepted-export/texture-budget check, Halloween material contracts, animation clip boundaries and the full asset audit pass alongside strict Clippy, Rust formatting and the locked all-feature release build. The final full asset audit reports zero errors across 411 models and 447 embedded PNGs; the Python source suite passes 373 tests with three optional native/package checks skipped. After the native material refinement, all 75 targeted asset tests pass again. Every final native capture reports fourteen loaded models (the twelve NPCs plus gallery fixtures), zero model failures, and the same physical drawable and simulation frame as its baseline. The local evidence directory retains the exact commands, screenshot receipts, hashes and complete audit. No gameplay or level sources are changed. Existing compiled gameplay packages require re-export with the new asset library; older binaries/packages are not an acceptance target for changed approved exports.

## Package integration

The changed NPC dependency closure affects three maintained gameplay projects.
Their content versions advance so existing installations can import a higher
release instead of encountering conflicting bytes under the same version:

| Package | Content version | Members | Exported lighting |
| --- | --- | --- | --- |
| Quiet Collection (`environments`) | 1.0.2 | 9 | Static |
| It Only Exists in the Beam | 1.0.1 | 4 | Static |
| One Guest Too Many | 1.0.1 | 1 | Static |
| Bundled menu | 1.0.0 | 1 | Static and Full |

The three gameplay exports are retained under
`local/npc-texture-refresh/integration/packages/`. They are ignored compiled
outputs, not Git source. All fourteen members pass dependency/integrity
validation and `verify --require-current` against their maintained projects.
The direct player imports all three exports into an isolated profile and lists
all fourteen members without a skipped package. Every member then loads in a
separate direct-binary Metal run with zero model failures or placeholder models.
The [native scene overview](../images/npc-textures-2026-11-03/integrated-maps.png)
records those fourteen Static scenes and the menu.

`assets/shared/menu/scene.bin` is regenerated from `projects/menu/` with the
current compiler and dependency identities. Its required Full lighting and
native reflection capture are prepared and verified; the actual main menu
loads with its Full atlas and reflections resident. The
[menu capture](../images/npc-textures-2026-11-03/integrated-menu.png) records that
normal application startup. Gameplay geometry, scripts, quest behavior and
unaffected release projects are unchanged. No gameplay Full lighting bake was
run. The current player/editor/compiler code is identical to the already
validated NPC binary build; only external packages and project release metadata
change in this integration.

These are Static-only gameplay development exports. The final release Levels
ZIP, which requires both Static and Full variants, remains future delivery work;
no release is published here. A future export with different content must also
use a higher content version if upgrading an installation of these exact
development packages. The package hashes, commands, current-source checks,
native receipts and isolated import profile are retained under
`local/npc-texture-refresh/integration/`.

To inspect the integrated game from this checkout with its disposable profile:

```sh
PLACES_ASSET_ROOT="$PWD" \
PLACES_STATE_ROOT="$PWD/local/npc-texture-refresh/integration/player-state" \
./local/npc-texture-refresh/runnable/places
```

## Image-generation prompts

### Creature materials

Use case: stylized-concept. Asset type: production low-poly game albedo material source atlas. Make a square 2048x2048 flat 2D texture image containing EXACTLY 4 columns and 4 rows of equal square material swatches, edge-to-edge with NO gutters, labels, frames, margins, objects, text, numbers or perspective. Every swatch must fill its entire 512x512 region. This is surface artwork to be cropped into existing PS2/Source-inspired NPC UV atlases; NOT a collage of rendered material balls. All swatches must have even neutral diffuse lighting with no vignette, ambient occlusion, directional light, strong shadows, outlines or large focal marks. Restrained hand-painted game texture style, fine but readable material strokes, subtle broad tonal variation; avoid photo noise and tiny high-contrast speckles. Exact swatch ordering left-to-right top-to-bottom: ROW 1: (1) charcoal-black short cat fur, dark slate with individually suggested soft fur tufts, midtone around #303238, never solid black; (2) warm ivory short cat fur #d7d3c7, subtle cream tufts; (3) muted brown-grey rat fur #645a4f, short directional strands and small warm tufts, no spots; (4) dusty muted rose animal bare skin #a7756d, delicate small creases. ROW 2: (5) aged warm ivory bone #c6bea3, faint pores and very sparse delicate hairline wear, NO black cracks; (6) weathered cool grey composite/concrete #a6a6a0, very subtle mineral pitting and rubbed unevenness, not dirty, no large cracks; (7) warm amber iris material #b57b25, delicate radial-like golden fibers but no whole eye or pupil; (8) muted dark rose animal nose leather #955b60, subtle tiny pores, no whole nose. ROW 3: (9) chestnut insect chitin #663b20, subtle longitudinal lacquer-like striations, low contrast, no specular glare; (10) harvest orange pumpkin rind #bf641d, close evenly spaced delicate vertical ridges and natural subtle mottling, no pumpkin shape; (11) dry dark umber pumpkin stem #4b3824 with irregular thin woody fibers; (12) nearly black soft matte recess #202023, smooth very subtle variation, no bright flecks. ROW 4: (13) aged warm off-white woven cotton sheet #ddd8c4, delicate visible weave with shallow irregular cloth creases, no large folds or deep shadows; (14) pale cool ivory woven cotton sheet #d5dfdb, delicate linen weave and subtle thin cloth creases; (15) grey-blue seabird feather texture #8c99a9, fine overlapping elongated feather barbs, subtle; (16) soft warm-white seabird feather texture #e0dfd7, fine overlapping elongated feather barbs. Cohesive polished authored material art for faceted models, recognizably retro but high quality. No transparency.

### Beach materials

Use case: stylized-concept. Asset type: production low-poly game albedo material source atlas. Make a square 2048x2048 flat 2D image containing EXACTLY 4 columns and 4 rows of equal square material swatches, edge-to-edge with NO gutters, labels, frames, margins, objects, text, numbers or perspective. Every swatch fills its entire 512x512 region. This is pure surface artwork cropped into existing PS2/Source-inspired animal UV atlases, NOT rendered objects or spheres. Even neutral diffuse lighting, no vignette, shadows or directional lighting. Restrained hand-painted retro game texture style with clear natural surface character, subtle broad tonal variation and readable small detail, avoid noisy photoreal speckles. Exact swatch order left-to-right top-to-bottom: ROW 1: (1) coral red crab carapace #e06049 with soft irregular rust red mottling and tiny sparse pits; (2) lighter warm coral shell #ef8060 with pale worn freckling; (3) deeper red-brown shell #b84b3c with subtle organic mottling; (4) coral red crab legs #d35a42 with faint fine lengthwise bands and creases. ROW 2: (5) golden lemon yellow fish scales #e6c247 with delicate small overlapping scalloped scales; (6) light golden yellow fish scales #f0d66b with delicate small overlapping scales; (7) muted turquoise fish scales #27a3ad with delicate small overlapping scallops; (8) deeper blue turquoise fish scales #298598 with delicate small overlapping scallops. ROW 3: (9) turquoise translucent-looking fish fin albedo #29a3b6 with fine almost vertical parallel fin rays, fully opaque image; (10) light aqua fish fin #65b6bb with delicate parallel fin rays, fully opaque; (11) warm ochre seabird bill keratin #dfaf42 with subtle fine lengthwise grain and light scuffing, no cracks; (12) ochre seabird scaly leg skin #c99738 with delicate small horizontal scale lines. ROW 4: (13) charcoal slate seabird flight feathers #4b566b with elongated close overlapping feather barbs; (14) light grey-blue seabird feathers #b2becb with elongated soft feather barbs; (15) warm off-white tiny feather down #e3e0d5 with very delicate feather tufts; (16) dark blue-black animal eye surface #19252b, almost flat with extremely subtle soft variation, NO painted whole eyeball, iris, pupil or highlight. No thick divisions or outlines. Materials remain clean and readable at 128-256 pixel game resolutions. No transparency.

