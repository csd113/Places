# Beach and Frutiger Aero implementation plan

Phase 1 discovery, October 9, 2026. Verified against `Art-style` at
`8a263b1c6914f3a570d3196faf8476080f396d85`, Rust/Cargo **1.99.0**, SDL3/wgpu.
This is a content implementation specification, not a claim that either demo
exists or has passed gameplay/visual certification. Phases 2–5 run sequentially.
Retain current HDR and baked precision; the separate
[storage investigation](compiled-map-size-investigation.md) and
[HDR audit](hdr-visual-value-audit.md) are outside this program's implementation scope.

## 1. Verified references and coverage

The two newest concept sheets by local modification time are the expected sets.
Both were inspected in full at original resolution by the primary and independent
art specialists. No additional Beach/Aero variants, theme READMEs or design notes
were found. Existing environment notes describe the older five themes.

| Reference, relative to repository root | Pixels | SHA-256 |
| --- | --- | --- |
| `assets/environment/Low-Poly Tropical Beach Concept Board.png` | 1536×1024 | `6f6f97f68890c15ddf5c5fa977757f62f28d4635865bffb87ef17286c846df03` |
| `assets/environment/Frutiger Aero Places Concept Sheet.png` | 1448×1086 | `4cc998ad3535219dd2d938c495f4274bad92712c5464715c618c0abc8d8bb34a` |

The sheets are currently **untracked user inputs**. Preserve their bytes and paths;
publish them unchanged with their corresponding implementation phase. Do not crop
the boards into shipping textures or mistake their preview skies for panoramas.
The following coverage keys must carry through asset integration, demo placement
and final certification. Scene-only landmarks count alongside labeled prop strips.

### Beach — sunny, colorful, relaxing

| Key | Required depicted content and acceptance details | Natural showcase location |
| --- | --- | --- |
| B01 | Cream faceted sand, gently curved shore, green ground behind palms, irregular white foam edge | Main beach and shallow entry |
| B02 | Cyan/turquoise shallows, deeper blue sea, broad clean polygon-cell water pattern | Shore and pier; submerged floor remains real geometry |
| B03 | Gray/blue-violet angular rocks and islands with thin green caps; prominent **sea arch** | Beach rocks and northwest offshore landmark |
| B04 | Tapered pale lighthouse, small dark/red windows, top gallery/lantern, **red conical cap** | Sea-arch island, visible from hero spawn |
| B05 | Bent, banded ochre palm trunk; separate pointed, folded green fronds, uneven drooping crown | Foreground framing and grassy fringe |
| B06 | Wood crate with thick frame, plank panels and diagonal **X braces** | Kiosk/pier supplies |
| B07 | Plank dock with substantial square posts and underside beams | Short walkable pier on right of hero composition |
| B08 | Open kiosk/hut: four timber posts, blue counter, broad yellow hipped roof, entry step | Beach end of pier |
| B09 | Larger blue waterfront stilt building, yellow roof, framed windows; secondary brown structure | Pier/town edge |
| B10 | Coral/off-white alternating umbrella sectors, pointed cap and slender dark pole | Sand seating vignette |
| B11 | Blue reclining sling chair with warm timber folding frame | Umbrella seating vignette |
| B12 | Timber signpost with right-pointing unlettered arrow | Beach/town route junction |
| B13 | Compact cream/blue/pale-coral town: yellow roofs, framed windows, arched ground-floor openings, exterior stairs, terraces/parapets | Connected small town behind east beach |
| B14 | Town shrubs/palms and orange/blue bunting | Courtyard and stairs |
| B15 | **Seagull: idle/fly**; white/gray body, dark wingtips, yellow beak/legs, black eyes | Perched pier bird and observable flying presentation |
| B16 | **Crab: walk/idle**; coral-red wide body, stalk eyes, angular legs, two pincers | Dry sand beside a clear observation route |
| B17 | **Fish: swim**; yellow face/bands, turquoise body/fins, dorsal fin, forked tail | Clear shallow water beside pier |
| B18 | Bright blue daylight sky, pale horizon, chunky faceted white/icy-blue cumulus and small sun | Seamless panoramic sky; real island geometry supplies parallax |

The board explicitly labels seven square surface families: **sand, water, rock,
stucco wall, wood plank, palm trunk, palm leaf**. Reconstruct all seven as real
PNG artwork; the printed 1024² labels describe the concept, while runtime asset
class contracts and measured visibility determine production resolution. Roof,
grass, upholstery and umbrella colors also need fitted/tiled resources where used.
Sunlight and cool shaded faces are depicted; no artificial Beach light fixture
or complex NPC behavior is required. Animation labels specify clips, not timing.

Palette reference samples: blue `#1f74c7`, cyan `#22acdc`, turquoise `#54d3d6`,
sand `#ead2a8`, ivory `#ede7db`, grass `#74b041`, lime `#bbd868`,
coral `#f46753`, yellow `#fed95d`. These are shaded-art samples, not baked
illumination or mandatory material multipliers.

### Frutiger Aero — nature, clean materials, optimistic space

| Key / sheet label | Required depicted content and acceptance details | Natural showcase location |
| --- | --- | --- |
| A01 / 01 | Broad rounded white frame with panel seams, cyan glazing, returning end and angular green leaf motif | Atrium glazing/bays |
| A02 / 02 | Wide faceted round-crown white portal, straight jambs, **lime inner band**, small cyan base insert, open center | Atrium/corridor threshold |
| A03 / 03 | **Three** tall cyan glass panels folded into a zigzag, thin white joints/rails, differently sized leaves | Lounge seating divider |
| A04 / 04 | Suspended wide **octagonal** white light pod, cyan luminous ring, pale recessed center, three suspension lines | Atrium feature; distinct from corridor's inset circular rings |
| A05 / 05 | Soft-corner square white planter, aqua base stripe, brown branched small tree, irregular angular green canopy, basal planting | Atrium corners and lounge |
| A06 / 06 | White curved wraparound shell, divided cyan two-seat cushions/back, recessed underside and short feet | Atrium and lounge seating |
| A07 / 07 | Tall framed cyan display on squat white plinth, lime side inset, leaf logo and “A BRIGHTER TOMORROW” | Entry information point |
| A08 / 08 | Shallow circular white basin with cyan water and **three stacked faceted cyan spheres**, green leaf detail | Central atrium sculpture |
| A09 / 09 | Walk-through cyan-glass tube with faceted barrel roof, white end arch frames and side leaf | Corridor/lounge connection |
| A10 / 10 | Upright cyan accent, broad diagonal white band and angular lime leaves; usable as floor/wall graphic | Reception feature panel |
| A11 / atrium | Glazed domed/skylit roof with pale radial ribs and circular soffit; tall segmented windows; white/aqua tile variation | Main atrium, open room ceiling plus authored roof GLB |
| A12 / corridor | Pale panel ceiling, repeated cyan ceiling rings, leaf glazing, planters, gray terminal double doors, wall typography | Connected corridor |
| A13 / reception | Curved white reception counter with aqua base strip, **two dark countertop terminals**, rounded overhead soffit, small warm downlights | Lounge/reception |
| A14 / graphics | Leaf branding; atrium “NATURE / PEOPLE / TECHNOLOGY / TOGETHER”; corridor “CLEANER / SPACES / BRIGHTER / TOMORROWS”; reception “PLACES / A BRIGHTER / TOMORROW / TOGETHER” | Fitted signage, banners and graphic panels |
| A15 / backdrop | Bright pale-blue cloudy sky, distant blue city towers and green landscape beyond glazing | Nontraversable scenic perimeter/backdrop |

Required material families: clean off-white ceramic/plastic paneling, aqua tile,
cyan glass, cyan upholstery, restrained silver trim, lime/mint graphics,
green foliage/brown branches and cyan water. Palette samples: white `#f5f0ea`,
aqua `#02b3e7`, sky `#38affc`, turquoise `#02b7a7`, lime `#6dca36`,
mint `#80d2ba`, silver `#95999f`. Preserve legible clean color blocks and
controlled highlights; no grime or photographic material treatment.

No animals, people, terrain kit, river or locomoting entity is depicted. The
header globe/bubbles/leaves are a supporting motif, already represented by the
sculpture and graphics. “Dynamic and calming” does not specify motion: a subtle
local sphere bob is an optional, documented interpretation, not a missing NPC.
The city is scenery, not a requirement to build a traversable metropolis.

## 2. Verified facilities, gaps and implementation decisions

Follow [map authoring](MAP_AUTHORING_GUIDE.md),
[asset contracts](ASSET_SPECIFICATION.md), [asset architecture](../assets/README.md)
and [desktop verification](VERIFICATION.md). Implementation wins over older
summary tables. Current geometry revision is **8**, solver **17**, cache **14**;
package major 1, schema 3, PLMP6/PLPF3 remain the baseline.

| Facility / actual implementation | Reuse and limits governing these demos |
| --- | --- |
| Catalog: `assets/catalog.json`, `src/assets.rs`, `src/loader.rs` | Add data themes `beach` and `frutiger_aero`, globally unique prefixed IDs; no theme restriction or Rust enum is needed. Neither family currently exists. PNG → catalog texture/material → map reference; GLB → catalog prop/entity → instance. |
| Geometry/tooling: `tools/props/mesh.py`, `glb.py`, `build.py`; `tools/entities/rig.py` | Use existing deterministic builders and closed geometry helpers. **Writer gaps:** prop materials emit OPAQUE/MASK only although runtime accepts BLEND, and the writer lacks NORMAL output. Phase 4 needs narrowly tested BLEND/opacity export; preserve intended faceting, adding optional normal export only if native inspection establishes a smooth-shading need. Preserve old output. |
| GLB: `src/gltf.rs`, `src/render/wgpu/material.rs` | Embedded PNG, multi-material primitives, optional normals, skin/rigid-node clips/morphs, scalar sheen and emission work. GLB props currently have **no probe/planar reflection routing or normal-map import**. Only `KHR_materials_emissive_strength` is accepted; transmission/IOR/clearcoat are unsupported. Use geometry, normals and sheen for curved glossy objects; do not promise refractive glass. |
| Architecture: `src/level.rs`, `src/collision.rs` | Rooms, open ceilings, regions, ramps/stairs, openings, arches, arc walls and pillars exist. Arbitrary GLB terrain is visual geometry, not a walkable floor. Roof profiles have no dome; author the glazed dome as a decorative GLB over an open-ceiling room. Hollow models use `solid:false` and explicit surrounding structural collision. |
| Lighting: `src/lighting/`, `src/render/common/light_transport.rs`, `character.rs` | Baked direct/indirect HDR and spatial irradiance probes exist. `global_illuminators` provide baked directional sun with visibility/softness; sky ambient is separate. Entities receive bounds-based spatial lighting and bounded practical direct lights. There are no realtime shadow maps. Animated character meshes do not cast their current pose into baked world shadows; use existing projected contact grounding and retain static shadow-important scenery. |
| Materials: `src/materials/`, `src/render/wgpu/world.wgsl` | Architectural normal maps, emission, specular/shine, probe reflections and one nearest visible half-resolution planar plane exist. Shine does not itself enable reflections. Emission does not create a light: pair the Aero pod GLB with explicit `props[].lights`. No new fixture family is necessary. |
| Transparency: `src/render/wgpu/world.rs`, `src/lighting/transport/alpha.rs` | Shared stable back-to-front batch/object sorting, straight alpha with depth testing/no blend depth writes; no intersecting-triangle sorting or refraction. Keep panes separated and inspect from both sides. `occludes:false` does not erase drawn triangles from prepared transport; actual MASK/BLEND governs light transmission. Avoid stacked nested glass shells. |
| Sky/atmosphere: `src/render/wgpu/sky.rs`, `src/environment.rs` | Real equirectangular panorama, brightness, separately authored sky ambient/color; distance/height fog and regional fog. Sky imagery does not supply sun illumination and is absent from reflection capture. Keep fog slight in these bright environments; far plane is 100 m. |
| Water: `src/render/common/water.rs`, `src/level.rs`, `src/game.rs` | Flat rectangle or 48-segment circle, wading/swimming/surfacing, separate floors/slopes/rims. Depth attenuation sets uniform volume opacity, not pixel-depth refraction. No waves/caustics simulation. Beach clarity/cells/foam come from real PNGs and geometry; Aero basin uses `swimming:false`. |
| Animation: `src/entities/`, `src/render/common/animation.rs`, `character.rs` | Named clips and explicit `play_animation` actions exist. Uncontrolled skinned placements follow player locomotion; uncued rigid clips hold bind pose. Ground routes snap to floor and cannot supply flying/swimming navigation. `animation.playing:true` alone does not emit a renderer cue: initialize gull/fish through an autostart timer action (`clip`, `loop:true`) with an animation component. Component speed does not affect named cues: author timing in clips. Crab routes provide walk/idle cues; avoid a permanent override. |
| Runtime/package: `src/compiler.rs`, `src/package/world.rs`, `src/loader.rs` | Compile off/medium/full variants before play. Runtime discovers every bundled `assets/levels/*.placesmap`; JSON is authoring only. The entire catalog hash is pinned by current packages: **even additive catalog edits stale existing packages**. Rebuild affected installed packages before tests/runtime/publication; never bypass identity validation. |

Useful existing construction references, rather than generic visual replacements:
`core:crate`, Outdoor closed boulders/ridge/bushes and `outdoor:collision_peg`;
Pool circular basin/recess and glass/water materials; Outdoor branching trees;
Home/Pool seating primitives. Existing pool water, muted glass, trees, furniture
and night sky differ visibly from these sheets. Keep them unchanged and author
local Beach/Aero versions. The existing material reflection path can serve
architectural glass/tile/water; do not overbuild a reflection system for spheres.

Asset locations: `assets/environment/beach/` and
`assets/environment/frutiger_aero/` with `textures/<surface>/`, `textures/sky/`,
`props/models/`, and fitted graphic/decal resources. Beach rigs belong under
`assets/entities/<animal-id>/` with source PNG, GLB and clip/placement README.
Use matching `beach:`/`frutiger_aero:` catalog namespaces. Preserve concept paths.
Register new `PROPS` modules in `tools/props/parts/__init__.py`; update blanket
builder exclusions for independently authored entities. Add new animals to the
entity validator and clip-boundary enumeration. Do not assume catalog inclusion
automatically expands those maintenance tools.
New builders should load committed PNG artwork, rather than regenerate imagery
on startup or blanket-rebuild unrelated hand-finished models.

Surface sheets must satisfy square/tiling/alpha contracts; fitted model atlases
normally use 256² native PNGs, embedded with equivalent standalone sources,
UVs in 0..1, +Y up/+Z front and appropriate base origins. Daytime skies are
seamless **2:1 POT equirectangular**, at most 2048×1024, top row at zenith.
Choose economical geometry that retains the distinctive silhouettes. Observe
existing differentiated prop/entity budgets in `src/level.rs` and the asset spec;
large facilities should be meaningful modules, not arbitrary cap-evading pieces.
Update the specification before introducing an uncovered asset class.

### Preserve the recent lighting correction

Read [the root-cause correction](model-lighting-root-cause-and-fix.md) before
changing shared lighting or geometry writers. Keep safe recovery ray origins,
source-primitive material identity and coplanar chart continuity, physical
footprint integration and scale-safe shader normal normalization. Small opaque
props receive High 32 / Medium 24 intervals/m; any transformed axis over 3 m
selects large-model High 2 / Medium 1.5, and cutout cards use 1. Keep rail/panel
detail in sensibly sized modules, inspect broad walls separately, and check bake
atlas overflow/fallback warnings. Full has at most eleven pages per contribution
group, bounded further by encoded group cost; Medium has eight. Never hide
smudges with brightness floors, model multipliers or indiscriminate ambient boosts.
Place pod emitter taps clear of the opaque housing: `occludes:false` does not
prevent triangle self-blocking. Inspect actual illumination below the finished pod.

## 3. Two composed, playable demos

The dimensions/positions below are chosen gameplay layouts, not measurements
from the illustrations. Finalize the source and then pin capture coordinates.
Use one authoritative generator per demo:
`tools/levels/build_beach.py` → `assets/levels/beach_demo.json`, and
`tools/levels/build_frutiger_aero.py` → `assets/levels/frutiger_aero_demo.json`.
Each gets `--check` deterministic source verification; sibling `.placesmap`
files are the shipped playable results.

### beach_demo

Approximately **48×50 m**, x −24..24, z −30..20; sea north (−Z), warm sand
south, grassy palm fringe inland. Hero spawn near `[-4,0,10]`, yaw 0°, slight
downward pitch: palm framing, bright shallows and open sand foreground,
lighthouse/arch northwest, pier and yellow-roof blue buildings right.
Keep landmarks inside the 100 m camera range and the shore uncluttered.

Main route: palm-framed beach → umbrella/chairs → kiosk/pier → small east town
courtyard → stairs/terrace → beach. Put supplies/signs near real destinations;
show every B-key naturally. Distant island/lighthouse provide the dramatic
silhouette; beach rocks give close inspection. Back/side terrain and town walls
hide the perimeter, with buried collision containment where necessary.

Build supported walkable sand, shallow bottom and dock deck using rooms,
regions and ramps; visual coastal meshes refine silhouette without becoming
fictional floor collision. Fit nearshore water below dry sand, with a gradual
supported entry/exit and actual submerged floor. Keep dock posts separate from
deck support so one big box does not block the pier. Test dry-to-wade-to-swim
and reverse, slopes, stair landings, structure openings, jumps and boundaries.
Player radius is 0.30 m, height 1.8 m; normal step 0.4 m, water exit step 0.5 m.

Crab uses a dry ground route with walk/idle pauses; separate gull instances show
perched idle and airborne wing animation; underwater fish shows tail motion and
small local turning/bobbing. Use `solid:false` for gull/fish. Explicit clips must
ignore player locomotion. **Clip translation does not move world bounds or light
anchors**: keep every posed vertex within the render culling envelope, bind half
extent ×1.15 +0.05 m per axis. Prefer rigid wing/tail nodes and verify all phases;
large bird or fish patrols would require a real bounds/anchor extension and are
not promised by this plan. Palms stay static unless optional sway's baked-shadow
limitation is explicitly accepted.

Use sunny directional illumination, cool sky fill, restrained fog and controlled
exposure. Align sky sun and light direction. Foam and broad water-cell artwork
must stay clean at Low. No physical sea simulation is required or claimed.

Pinned review views: **hero**, pier/hut, town/stairs/arches, shallow foam/fish,
lighthouse/sea arch, inland palms/terrain, and close animal animation views.
Test identical view/FOV/time at Low/Medium/High plus settled quality restoration.

### frutiger_aero_demo

Single floor y=0, about **32×12 m**: atrium x=0..12/z=0..12, 5.2 m volume;
corridor x=12..22/z=4..8, 3.4 m; lounge x=22..32/z=1..11, 3.4 m.
Hero spawn near `[2,0,10]`, looking toward the three-sphere fountain at `[6,*,6]`:
white/cyan floor, bright tree/bench framing, glazed ribbed dome and skyline depth.
Preserve airy circulation around an approximately 3 m fountain.

Explore atrium perimeter seating → lime arch → lit leaf-glass corridor → canopy
→ lounge/reception and secondary divided seating. Kiosk at entry, folded
partition by seating, accent/banner behind curved counter, terminals on counter;
planters reinforce bays without blocking the route. Include every A-key.
Corridor ends in a framed threshold/double-door presentation into the lounge,
rather than an isolated display room. Exterior blue towers/greenery and sky
finish window views without expanding the playable city.

Build real structural floors/walls/openings with the current map format; roof,
counter, rounded frames and canopy are dedicated models. Glazed passages use
split frame collision or supported arch/column/solid-pane primitives. Generic
solid boxes must not close portal holes. Inspect collisions against visible
frames, benches, planters, reception and fountain rim from both sides.

Use soft daylight, restrained glossy tile/white panels, saturated controlled
cyan/lime, cyan pod light and warmer lounge downlights. Gloss/BLEND must earn
their cost through native comparison. Use opaque glossy faceted spheres as the
robust initial sculpture, with real blended water; inspect against the reference
before considering translucent sphere surfaces. The final spheres must read as
bright glass-like bubbles; blue stone/solid-ball appearance fails art review and
requires refinement. Avoid nested glass shells.
Small optional sphere motion uses existing clip machinery and is
documented as interpretation. No Aero creature animation is required.

Pinned review views: **atrium hero**, corridor light rhythm/signage, reception,
close glass/leaf/skyline, lime arch, canopy/partition, and sculpture/material
detail. Review dome transparency from inside/outside and at crossing angles;
include High/Medium/Low and independent reflections-off for honest fallback.

## 4. Sequential ownership and phase gates

Primary alone owns Git/index, catalog integration, shared toolkit files,
authoritative docs, compiler output and native runs. Asset specialists own
disjoint theme-specific builder/PNG/GLB subtrees; entity specialist owns only
new Beach rig subtrees. Integrate their completed work before builds. Reviewers
remain read-only. Run one heavy build/bake/benchmark at a time with at most
12 workers. Never change branch, touch main, overwrite other work, or force-add
ignored output. Preserve the untracked storage/HDR reports and experiments.

| Phase | Dependencies, owned outputs and completion gate |
| --- | --- |
| 1 — discovery | This independently reviewed document only. References hashed/visually inspected; source claims and commands verified; no production edits/builds or claimed native acceptance. Commit/push on Art-style. |
| 2 — Beach assets | Read Phase 1. Split static models, seven surface families/sky/foam, and three animal rigs. Primary integrates catalog/spec/docs and any necessary narrow authoring support. Cover B01–B18, including scene-only architecture/landmarks. Validate/export actual assets and preview through native tooling where available; update generated Model Zoo and current bundled packages after catalog freeze. Commit/push working integration. |
| 3 — Beach demo | Consume Phase 2 asset/clip coverage. One map owner builds the connected scene; primary owns compilation/captures; read-only art reviewer compares the board. Require genuine load/exploration, reliable movement/collision, complete coverage, coherent lighting, reviewed hero/secondary captures and resource measurements. Correct before commit/push. |
| 4 — Aero assets/demo | Consume earlier contracts/corrections. Split architecture, props/graphics, textures/sky; primary owns BLEND writer integration, catalog and composed map. Cover A01–A15, rebuild bundled dependencies after freeze, then native material/collision/composition refinement. Require a playable visually reviewed demo and measured overhead before commit/push. |
| 5 — corrective certification | Independently review both full routes against references, correct omissions/defects, repeat meaningful captures/tests, check animation/proximity lighting and hero regression, finalize measurements/coverage and one consolidated implementation report. Keep prominently named hero PNGs per theme. Clean only disposable task outputs; final commit/push and exact remote/CI verification. |

Catalog changes require deliberate package publication in Phases 2 and 4.
Regenerate `model_zoo.json` from its catalog-driven generator, then rebuild
bundled packages. Add explicit ceiling/water/table/wall classifications for new
Zoo displays, and avoid ID collisions after its colon-to-hyphen conversion.
Expand the exact bundled-ID assertions in `tests/test_package.py`. Add demo
generator checks to `tools/verify.sh`, and one-view inventory manifests named
`assets__levels__beach_demo.json` and `assets__levels__frutiger_aero_demo.json`
under `tests/fixtures/native/map-manifests/` for the all-map native gate.
Preserve historical custom packages; rebuild only active
installed custom packages against current assets, or review through their
preserved matching asset roots. `tools/verify.sh` additionally inventories all
supported sources for the full gate; this discovery phase does not run it.
All ten currently installed archives carry catalog provenance. Build collections
in `assets/levels` and `levels` sequentially after freezing the compiler/catalog;
preserve custom sources and prior runnable evidence. Current dependency closure
omits an explicit decal-sheet loop: force rebuilds for same-path decal PNG edits
or repair that narrow closure with regression coverage. Rebuild the final player
after bundled-package updates (Places Demo is embedded), then confirm the
compiler executable identity stayed unchanged, as the desktop gate does.
New `.placesmap` sizes must be measured before staging. Use repository Git LFS
conventions where large new archives require them, without storage-format edits.

## 5. Executable validation and honest evidence

Commands below were verified from current scripts/source; they are **future
implementation gates**, not Phase 1 test results. From repository root:

```sh
cargo build --release
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 tools/assets/audit.py --workers 12 --out tools/bench/results/environment-program/assets.json
python3 tools/entities/rig.py --check <new-entity.glb>
python3 tools/entities/check_clip_boundaries.py
python3 tools/entities/validate_entities.py --workers 8
python3 tools/levels/build_model_zoo.py --check --no-cache
python3 tools/levels/build_beach.py --check
python3 tools/levels/build_frutiger_aero.py --check
./target/release/places --check-geometry --level assets/levels/beach_demo.json
./target/release/places --check-geometry --level assets/levels/frutiger_aero_demo.json
./target/release/places-compile build-collection assets/levels --workers 12
./target/release/places-compile build-collection levels --workers 12
./target/release/places-compile validate assets/levels/beach_demo.placesmap
./target/release/places-compile verify assets/levels/beach_demo.json --package assets/levels/beach_demo.placesmap --require-current
./target/release/places-compile validate assets/levels/frutiger_aero_demo.placesmap
./target/release/places-compile verify assets/levels/frutiger_aero_demo.json --package assets/levels/frutiger_aero_demo.placesmap --require-current
cargo build --release --bin places
./target/release/places --list-levels
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
sh tools/check-rust.sh
python3 -m unittest tests.test_package
git diff --check
```

Angle-bracket operands are substitutions; run only commands for assets/maps
already introduced by that phase. New generators/manifests are planned outputs,
not currently available files. The entity sweep is currently a maintained
enumeration: extend it and clip checks to include the new animals and their
flight/swim envelopes instead of assuming a global check includes them.
Focused relevant tests include GLB/material/prop tests, coplanar chart and
coverage regressions, model-lighting normal/recovery tests, and real controller
routes for both demos. Use [native model-lighting controls](../tests/fixtures/native/model-lighting/README.md)
for approach/retreat and quality restoration; do not claim CPU tests exercise
fragment normals. Shared changes also require the existing Art-style hero views.

Phase 5 additionally runs the authoritative desktop gate and its separate
native inventory smoke campaign, without a visual overhaul of old environments:

```sh
sh tools/verify.sh
python3 tools/bench/native_regression_maps.py --campaign <completed-package-campaign> --out <new-native-directory>
```

Use the completed package campaign produced by the desktop gate; do not rebake
it solely to run the native smoke campaign. Preserve and compare compiler
SHA-256 before/after the final player rebuild.

Create functional camera manifests alongside `tests/fixtures/native/hero-manifest.json`
for each new demo. Reuse the native capture tool, for example:

```sh
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/beach-manifest.json --quality high --out tools/bench/results/beach-final/high
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/frutiger-aero-manifest.json --quality high --out tools/bench/results/aero-final/high
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/beach-manifest.json --quality high --views hero --frames 600 --finish-gpu --out tools/bench/results/beach-final/performance
PLACES_ASSET_ROOT="$PWD" PLACES_LEVEL=beach_demo ./target/release/places
PLACES_ASSET_ROOT="$PWD" PLACES_LEVEL=frutiger_aero_demo ./target/release/places
```

Pin FOV 60°, verified 1280×720 drawable resolution, exposure, quality, simulation
time and camera in manifests. Capture Low/Medium/High in separate fresh output
directories; use real held-controller scripts/state logs for traversal. A camera
eye override alone is not proof that the player can reach that location.
Keep final genuine, unchanged screenshots under
`docs/images/two-environments/beach/` and `frutiger_aero/`, with `hero.png` in each.
Image grids/differences are supporting diagnostics, not replacements for normal
native-image inspection and a real playable route.

Measure optimized release runs sequentially with a visible, unoccluded desktop.
Reject locked/minimized/occluded timings; report unavailable presentation and
carry that verification forward. Separate capture readback from performance;
`--finish-gpu` is completion timing, not GPU timestamp profiling. Record median
frame/load/bake time with settings and package/binary identities. Use
`python3 tools/bench/scene_budget.py --package <map> --log <native-log> --variant <off|medium|full> --out <new-json>` for actual
draw/triangle/resident inventories; it excludes some passes and does not measure
transparent fragment overdraw or total GPU memory. Inspect package bytes, atlas
occupancy/fallback and decoded texture memory. Compare against the existing hero
at the same settings; optimize accidental duplication or clear bottlenecks,
never trade important appearance for marginal theoretical savings.

For loading and clean compile measurements, use the maintained tools with fresh
output directories (the compiler repeat intentionally measures forced bakes):

```sh
python3 tools/bench/loading.py --binary target/release/places --levels beach_demo frutiger_aero_demo --repeat 3 --out <new-directory>
python3 tools/bench/compiler_bench.py --binary target/release/places-compile --maps assets/levels/beach_demo.json assets/levels/frutiger_aero_demo.json --workers 12 --repeat 3 --out <new-directory>
```

Match scene-budget variants to actual quality: Low → off, Medium → medium,
High → full; the default full cannot certify a Low/Medium log.

## 6. Phase 1 outcome and remaining verification

Both reference sets, major asset families, scene landmarks and animation labels
are accounted for. The plans reuse current architecture and identify actual
writer, geometry, reflection, water and animation limitations. Phase 1 changed
documentation only; no asset generation, map compile, native runtime capture,
gameplay certification or performance measurement was attempted. No native
presentation limitation was tested in this phase. All final completion-gate
claims remain for the sequential implementation phases, with any blocked test
reported explicitly rather than marked complete.
