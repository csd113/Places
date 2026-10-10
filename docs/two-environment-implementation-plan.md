# Beach and Frutiger Aero final program report

Final Phase 5 integration on `Art-style`, October 10, 2026. Both demos are playable, independently reviewed against their concept sheets and refined; all local final gates passed.
Both original concept sheets were inspected at original resolution by the primary
and independent visual reviewer. Independent lighting/material and traversal/technical
reviews also accepted the final demos. All B01–B18 and A01–A15 families are implemented
and naturally placed. This report consolidates all five
phases; the authoring and asset contracts remain in their canonical guides.

## Playable deliverables

| Demo | Authoring source | Shipped playable package | Generator |
| --- | --- | --- | --- |
| Beach | `assets/levels/beach_demo.json` | `assets/levels/beach_demo.placesmap` | `tools/levels/build_beach.py` |
| Frutiger Aero | `assets/levels/frutiger_aero_demo.json` | `assets/levels/frutiger_aero_demo.placesmap` | `tools/levels/build_frutiger_aero.py` |

Beach is a connected 48×52 m cove with sand, submerged support, pier, hollow
waterfront buildings, town arches/stairs and a planted inland return. Aero connects
its 12×12 m atrium, 10×4 m corridor, 10×11 m reception and 32×8 m garden terrace.
Both use real structural support and tight local collision around hollow models.
Scenery boundaries keep landmarks within the 100 m camera range.

After the authorized final clean, use the retained ordinary player:

```sh
player=tools/bench/results/environment-program-20261009/phase5-final-tools/places
PLACES_ASSET_ROOT="$PWD" PLACES_LEVEL=beach_demo "$player"
PLACES_ASSET_ROOT="$PWD" PLACES_LEVEL=frutiger_aero_demo "$player"
```

For a fresh checkout, `cargo build --release` recreates `target/release/places`.

The final ordinary tools and self-contained runnable snapshots are preserved
outside `target` in `tools/bench/results/environment-program-20261009/phase5-final-tools/`
and `phase5-final-runnable/`. That ignored evidence queue also holds manifests,
exact package/tool identities and recoverable raw evidence, rather than duplicating
large audits in the public documentation. The separately supplied storage/HDR
audits and all nine unrelated inputs remain unchanged.

## Reference coverage and assets

| Immutable concept | Pixels | SHA-256 |
| --- | --- | --- |
| `assets/environment/Low-Poly Tropical Beach Concept Board.png` |1536×1024|`6f6f97f68890c15ddf5c5fa977757f62f28d4635865bffb87ef17286c846df03`|
| `assets/environment/Frutiger Aero Places Concept Sheet.png` |1448×1086|`4cc998ad3535219dd2d938c495f4274bad92712c5464715c618c0abc8d8bb34a`|

The [Beach kit](../assets/environment/beach/README.md) contains 27 static GLBs,
three skinned animal families, five clips and 29 retained PNGs. Its 52 catalog
resources comprise 30 placeables, 11 textures and 11 materials; asset geometry totals
7,786 triangles. Seven labelled surface families—sand, water, rock, stucco, wood,
palm trunk and palm leaf—plus roof, grass, foam, upholstery and umbrella resources
are committed artwork. Source paths are `assets/environment/beach/` and
`assets/entities/beach_{seagull,crab,fish}/`.

### Beach

| Key | Implemented reference content | Natural showcase location |
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

The [Aero kit](../assets/environment/frutiger_aero/README.md) contains 23 GLBs
(6,864 triangles), 26 PNGs, six materials and five reusable decal entries. Clean
white paneling, aqua tile, cyan glass/upholstery/water, restrained silver trim,
lime graphics and green foliage retain the depicted silhouettes and fitted UVs.
The below-grade scenery foundation reuses the committed Beach grass PNG.

### Frutiger Aero

| Key / sheet label | Implemented reference content | Natural showcase location |
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

All imagery loads from real committed PNGs. Surface tiling, fitted graphics,
alpha, aspect ratios, +Y-up/+Z-front orientation and model base/UV contracts follow
[ASSET_SPECIFICATION.md](ASSET_SPECIFICATION.md). Both daylight panoramas are
seamless 2:1 equirectangular skies with cloudy faceted artwork; the authored sun
matches the panorama bearing/elevation. No concept-board crop is used as a texture.

## Animation and rendering contracts

Beach explicitly initializes gull `idle`/`fly`, crab `idle`/`walk` and fish `swim`.
The walking crab follows a one-metre sand route at approximately 0.16 m/s with idle
pauses and a turn/return. The other flight/swim clips animate locally around fixed
world anchors; they are bounded presentations, without airborne/swimming navigation.
Full 120 Hz pose sweeps preserve all XYZ culling envelopes, with minimum margin
57.7 mm for gulls, 74.3 mm for crab and 64.8 mm for fish. Underwater fish keep more than
0.52 m clearance over the real seabed throughout their clip.

Baked directional sunlight, soft visibility, direct/indirect HDR transport,
spatial irradiance probes, projected character grounding, reflection probes,
water tint/attenuation, slight fog and optional bloom use the existing renderer.
The Aero fixtures retain legitimate housing shadows and explicit local emitters.
World material normal maps and current model-lighting precision are preserved.
Exposure/ambient settings, global precision and production Rust renderer are
unchanged in Phase 5; skeleton/kitchen/corridor fixes remain protected.

Phase 2 corrected the panorama U-wrap derivative seam. Phase 3 corrected world-Y
versus seabed-relative authored light positions and Low sunlight depth handling.
Phase 4 added tested GLB BLEND/opacity authoring while preserving legacy outputs.
Phase 5 corrects environment content without a movement or lighting-engine rewrite.

## Final corrective work

- Beach’s coarse 1/8 m shore joints drew conspicuous transverse sand bands. The
  central curved shore now uses capped outer tangents and binary-exact 1/64 m joins,
  with 64 supported bands, maximum 9.375 mm upper / 25 mm submerged lateral risers and
  one continuous seabed room. The new regression checks at least 50 actual joins
  on each grade. All 303 existing solid proxy transforms remain unchanged.
- Foam formerly inherited narrow collision-band scale and became a ruler-thin
  line. Sixteen independently placed broader fitted ribbons restore the irregular
  white silhouette without adding geometry/texture assets. A reachable wider town
  view now shows the stairs, arches, yellow roofs, bunting and shrubs together.
- The glass sphere stack was smoky gray. Icy-cyan/white facets and .45 BLEND alpha
  preserve the geometry and leaf detail with compact scalar sheen. The material’s
  roughness .055 and metallic .12 are stylized authoring inputs, not full refraction.
- Aero ceiling mottling was real prepared light, rather than albedo, flipped
  normals or reflection noise. Existing direct/indirect/filtered/filled stage
  diagnostics traced it to fixture-support/fill patterns. Ring housing is physically
  recessed into the ceiling; eight small square rect emitters per ring lie within
  its actual faceted luminous annulus and avoid the opaque centre. Sampled ceiling
  variation improved; mild residual baked variation remains. No brightness floor,
  ambient boost, disabled shadow or global quality reduction was introduced.
- Green banks previously ended against sky below the glazing, including east
  reception and corridor garden views. One closed nonsolid/nonoccluding scenic
  foundation joins the landscape, 6 cm below every playable floor. Replacing adjoining
  boxes also removed 16 buried coincident-face checker warnings at their source.

Rejected dense shore/room-partition and fixture candidates are retained recoverably;
atlas fallback or missing probe coverage was never accepted as a final bake.

## Curated native screenshots

These 17 PNGs are unmodified 1280×720 ordinary native wgpu captures, independently
inspected against the original sheets. Old phase screenshots are recoverable in
Git history and the evidence archive. `hero.png` identifies each promotional view.

| Beach | Aero |
| --- | --- |
|[Promotional hero](images/two-environments/beach/hero.png)|[Promotional hero](images/two-environments/frutiger-aero/hero.png)|
|[Shoreline/water](images/two-environments/beach/shoreline.png)|[Atrium architecture](images/two-environments/frutiger-aero/architecture.png)|
|[Sea arch/lighthouse/sky](images/two-environments/beach/sea_sky.png)|[Water/sculpture](images/two-environments/frutiger-aero/water.png)|
|[Waterfront structures](images/two-environments/beach/structures.png)|[Corridor/materials/lights](images/two-environments/frutiger-aero/corridor.png)|
|[Town/stairs/bunting](images/two-environments/beach/town.png)|[Reception/terminals](images/two-environments/frutiger-aero/reception.png)|
|[Inland garden](images/two-environments/beach/garden.png)|[Seating/glazing](images/two-environments/frutiger-aero/seating.png)|
|[Gulls](images/two-environments/beach/gulls.png)|[Kiosk/graphics](images/two-environments/frutiger-aero/kiosk.png)|
|[Crabs](images/two-environments/beach/crabs.png)|[Garden terrace](images/two-environments/frutiger-aero/terrace.png)|
|[Fish](images/two-environments/beach/fish.png)| |

## Final verification

`RUSTC_WRAPPER='' CARGO_BUILD_JOBS=12 sh tools/verify.sh` passed on the frozen
final inputs. Its Rust gate ran `cargo fmt --all --check`,
`cargo check --locked --workspace --all-targets --all-features`, debug and release
`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
(the release invocation adds `--release`) and
`cargo test --locked --workspace --all-features`: **2,205 library + 7 other tests
passed**, zero failed. Separate `cargo test --locked --workspace` passed
**2,199 library + 7 other tests**. Each ordinary run reported 32 ignored cases;
the required ignored atlas case and two native Low-lighting cases were then run
explicitly and passed. The other **29 ignored cases were not run**.

The same desktop gate passed asset validation, 12-worker inventory, GLB/clip checks,
all generated-source checks and `python3 -m unittest discover -s tests -p 'test_*.py'`:
**330 passed, zero skipped**. `regression_maps.py --run-packages` inventoried
52 sources: all 50 supported maps passed current-package validation, one intended
CPU-boundary witness and one named loader rejection behaved as expected.
`python3 tools/bench/native_regression_maps.py --campaign
target/verification/map-regression-20261010T140437Z-8745 --out
tools/bench/results/environment-program-20261009/phase5-final-inventory-native
--binary tools/bench/results/environment-program-20261009/phase5-final-tools/places`
then loaded/captured **all 50 supported maps**, with zero failures. All **12 installed
packages** passed `places-compile verify <source> --package <package> --require-current
--json`, `places-compile validate <package> --json` and independent ZIP CRC checks.
The final inventory helper's single-root glob initially omitted five installed
packages; completing just those five resolved its count assertion without repeating
successful suites or hiding a test failure.

Actual held-controller routes covered **35 Beach routes / 1,265 samples** and
**20 Aero routes / 819 samples**: walking, jumps, slopes, deep/shallow water entry
and exit, pier return, doorways, furniture and scenery boundaries. All **55 routes /
2,084 samples** passed. Aero endpoints and collision data preserve Phase 4 exactly;
all five exported Beach clips initialize and play. Two additional native hero
captures using only the retained snapshot asset root verified cleanup-safe playability.

Final integration inspected 35 High views and 24 selected Medium/Low views, plus
live quality restoration, bloom off/on/off, approach/retreat and timed animal
poses. Established Hero’s six matched High views are byte-identical to Phase 4
with the exact bloom-on, frame 60, fixed 1/60 protocol. Aero reciprocal controls
restore identical pixels. Beach animation differences are confined to actual
moving animals; fixed-anchor spatial payloads remain stable across camera moves.
These captures verify sampled behavior, rather than every possible input/pose.

The source geometry checker reports zero errors for both demos: Aero zero warnings;
Beach one tiny 0.00025 m² opposed-face overlap at the submerged pier/shore closure
(run 57.3 mm, maximum 8.6 mm height), with no overlapping walking top or new collider.
Existing narrow intent annotations describe genuine kit boundaries and buried
shore caps. Normal final package bakes have no warnings or atlas fallbacks.

## Measured costs

Measured on Apple M2 Pro / Metal, 12 logical CPUs, 16 GiB, macOS 27.0.1 (26A434).
Quiet serial ordinary-release runs used a 640×360 logical / 1280×720 drawable
window, VSync/bloom off, fixed 1/60 simulation, GPU completion waits, 120 warmup
and 600 measured frames, three repeats per row and retained OS/GPU caches.
All 18 runs had nonzero submissions and at least 720 actual ready presents;
locked, occluded or unpresented results were not accepted.

| Scene / quality | Median loop ms | P95 loop ms | Draws / submitted triangles | Median process peak RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| Beach hero / High |5.689|15.896|89 / 19,466|1,086.8|
| Aero hero / High |6.579|14.093|83 / 18,546|593.3|
| Aero corridor / High |6.555|14.848|62 / 8,468|595.9|
| Existing Hero room / High |5.730|14.938|31 / 4,954|396.5|

Aero hero Low / Medium loops were 5.950 / 5.526 ms, P95 15.383 / 15.260 ms;
scene targets were 480×270 / 640×360, with the same 1280×720 drawable. Their
process peak RSS medians were 156.3 / 580.7 MiB. Whole-update means were
1.499 ms Beach and .076 ms Aero High (medians of repeats); these include ordinary
simulation and animation work, rather than isolated clip costs. Beach retains
74 animated joints / 8,628 animated vertex slots. Native world batches contain
13 translucent Beach / 2 translucent Aero ranges; fragment overdraw is unmeasured.

`python3 tools/bench/loading.py --binary
tools/bench/results/environment-program-20261009/phase5-final-tools/places --out
tools/bench/results/environment-program-20261009/phase5-final-loading --levels
beach_demo frutiger_aero_demo --repeat 3 --window-size 640x360 --timeout 120` produced 12 actual presented
High scene starts. Median first usable scene was **2.199 / 2.178 s Beach** and
**1.644 / 1.625 s Aero**, empty / warm application cache respectively; OS/GPU
caches remained intact. These include process startup and GPU upload.

| Final package / normal bake | Beach | Aero |
| --- | ---: | ---: |
| Compressed `.placesmap` bytes |43,285,251|26,349,811|
| Compile seconds, all normal variants |334.699|126.659|
| High atlas pages / resident MiB |9 / 144|4 / 64|
| Medium atlas pages / resident MiB |7 / 112|4 / 64|
| High world textures / decoded prop source MiB |6.667 / 7.504|16.000 / 5.754|
| High VBO / index bytes |5,473,064 / 158,688|4,839,376 / 141,804|
| Spatial probes / valid probes |4,864 / 81|13,760 / 1,142|

Final bakes froze catalog/source and retained geometry/preparation caches, used
12 transport workers, and preserved normal Off/Medium/Full quality, probe coverage
and deterministic provenance. Both retained two reflection-probe locations.
Aero's final package is larger than Phase 4's 18,485,830 bytes; accepted visual
corrections were preserved instead of reducing bake quality for size savings.

Draw/triangle counters describe base-scene submissions, including depth-occluded
ranges; sky, emission, post-processing and UI are excluded. GPU-wait loop time
includes completion/presentation work and is not an isolated GPU timestamp,
display FPS guarantee or cross-platform comparison. Resident resource inventories
are separate allocations, rather than total GPU/driver memory. No theoretical
savings are reported.

## Remaining limits

Fine stylized shore/foam joins and some bank contact edges remain visible at close
range, especially Low. Fish are subdued by water attenuation. Gull flight and fish swimming remain
anchored clips; current animated poses do not cast realtime world shadows.
Aero retains mild baked ceiling variation. GLB glass uses compact sheen and
stable object/batch alpha sorting without refraction, per-triangle intersecting
sorting, probe/planar routing or imported normal maps. Architectural reflections
use the existing material path. Aero’s sheet requires no animal family or NPC.

GPU timestamps, true fragment overdraw, isolated animation/transparency costs,
same-position dynamic/static lighting parity and other native backends/platforms
remain unmeasured. Those are stated instrument limits; no such measurements are
claimed. No critical visual, traversal or runtime blocker remains in the accepted final samples.

## Phase identities and publication

| Phase | Accepted commit |
| --- | --- |
|1 — discovery/contracts|`ef219f23e2d4e3b13708c81a8ec0b5d629d1070c`|
|2 — Beach assets|`1bd47995930d6c3eef8c9e029f7a4eded20e231b`|
|3 — playable Beach|`572decd76f7b577ab1a30be8a5dec50864bad4cf`|
|4 — playable Aero|`c840889ed9734e8eb3b0d0672db652700e00e19b`|
|5 — corrections/certification|`5d93828c1c4f4f93823fb5e340ba5f3b0bff2d7e`|

Phase 5’s report-publication commit, exact remote SHA and terminal CI are recorded
in the final delivery receipt. All work stays on `Art-style`; no main merge or
release is part of this program. After terminal publication acceptance, disposable intermediates and unique debug
maps are archived recoverably with file hashes before removal; the authorized
`cargo clean` runs only after a fresh ownership check. Final runnable snapshots,
ordinary tools, curated images and essential evidence remain outside `target`.
The delivery receipt records cleanup and released custody.

Canonical contracts: [map authoring](MAP_AUTHORING_GUIDE.md),
[asset specification](ASSET_SPECIFICATION.md), [desktop gate](VERIFICATION.md),
[model-lighting correction](model-lighting-root-cause-and-fix.md).
