# Winter — concept reconstruction, 2026-10-08

Starting revision: [`026eedb`](https://github.com/csd113/Places/commit/026eedb1995669e3c8951c1da447d88edc0fe316), on the existing `Winter-expansion` branch after the completed Outdoors pass. This is the fourth entry in the [sequential visual journal](../README.md). [Reconstruction history](https://github.com/csd113/Places/commits/Winter-expansion/docs/style-upgrade-20261007/winter) locates `rebuild winter assets toward concept art`; the final task handoff records its exact remote SHA and CI.

The actual immutable [Winter Expansion Environment Concept Sheet](../../../assets/environment/winter/Winter%20Expansion%20Environment%20Concept%20Sheet.png) was inspected before authoring. Its SHA-256 remains `484921678b5f56203f3790f7cb5f9b4a83ae8a8b0dfb48c76aadc43c11a45550`. Prior Pool, Home and Outdoors journal text, images and review payloads remain intact.

## Reference and baseline

The sheet uses low-poly tiered evergreens with ragged dark skirts and broad thick snow loads. Snow is pale lavender and blue-grey, with large economical facets rather than granular noise. Exposed boulders remain dark beneath broad white crowns. The architectural vignette shows rectangular grey mineral courses, a deep timber-framed entrance, a projecting hood with diagonal knee braces, warm lanterns, string bulbs, stone steps/parapets and stout piers. Timber rails carry thick exposed caps. Varied icicles descend from ledges. Frozen water has angular blue plates and pale fracture lines. Green/cyan/violet aurora rises behind real dark mountain silhouettes; calm snow and severe whiteout are both shown. Warm local details pull the eye through a mostly cool scene.

The starting playable settlement already had three cottages, a raised lodge/deck/stair/ramp, cleared walking lines, an ice pond, forest, snow banks, icicles and working weather. Its cottage exteriors were plain shared siding, window framing was primitive, snow texture fine/noisy, ice cracks restrained, and the snow trees/rocks still embedded the preceding Outdoors base generation. The warm bulbs were functional. This pass reconstructs the mineral/timber village language and snow forms around the established routes.

## Concept-object coverage

| Depicted object / family | Decision and actual playable coverage |
| --- | --- |
| Broad faceted snow / packed walking lines | **Remade artwork.** New authored 1024² snow master and periodic production PNG, downsampled to native 256² for models. Existing 8 m snow and 5 m packed repeats, floor ownership, paths, traction and terrain heights retained. |
| Roof loads / eaves / exposed ledges | **Retained supported geometry, retextured.** Existing pitched caps, irregular eave lips and exposed cottage sills now use the new snow artwork. Native roof stock remains exposed at ridges/fascias. Lodge deck, porch rails and sills beneath its sealed awning stay dry. |
| Grey stone building / doorway / foundation | **Rebuilt presentation.** All exterior wall faces, raised foundation skirts and stair risers use new slate masonry; interior finishes stay domestic. Three new coursed open `entrance_frame` models replace shared doorway panels and corner dressing. Real operable leaves, openings and footprints are preserved. |
| Timber door / deep window trim / warm glass | **Refined/rebuilt.** Winter-local board material on existing leaves/frames and structural rails. Six new `window_frame` models have deep sills, recessed jambs and cross mullions around the actual glass openings. A restrained amber glass material preserves blend and local warm/cold balance. |
| Braced projecting snowy entrance hood | **New.** Two `door_hood_snow` models at exposed cottages replace flat primitive covers and old overlaid caps. Closed timber roof, two diagonal braces and thick exposed nose accumulation; sheltered back stays bare. Existing `snow_door_overhang` remains in the reusable library and Zoo. String attachments project ahead of the hood and icicle roots. |
| Stone approach walls / parapets / piers | **New.** Eight coursed `stone_wall_snow` sections flank the clear forest approach; eight `masonry_pier_snow` placements frame path and square. Real stone courses, joints, foot/coping and thick snow crowns. Narrow body colliders leave the established central walking spine open. |
| Timber lantern posts / warm exterior fixtures | **New + retained.** Two `timber_lantern` posts replace the tall metal square fixtures with timber brackets, tapered amber panes and snow-loaded hats. Existing source intensity/range stays 1.5/14 m. Two retained Outdoors fence lanterns sit on new path piers with small owned local lights. Completed wall/garden lamp models are retained; coarse fixture self-occlusion is disabled without raising their original light strengths. |
| Evergreen / trunk / branches / exposed green skirts | **Remade snow variants.** Current 708-triangle canonical Outdoors evergreen imported exactly, including art/UV/material records. Connected closed upward bough coats replace scattered triangle snow. Light/heavy totals are 1,284/1,380 triangles; selected whole tiers and undersides remain dark. Existing trunk colliders and 61-tree distribution remain. |
| Boulders / broken rock faces / thick caps | **Remade snow variants.** Current completed Outdoors mineral geometry and 256² atlas imported exactly. Snow crowns follow actual shoulder intersections, sink into support and leave mineral sides exposed. Bare/shared rocks are retained. Perimeter collision remains intact. |
| Terrain banks / asymmetric drifts | **Remade.** All six drift/mound families use closed unequal ten-segment rings, offset crowns and lower 60-triangle cost. Wall/fence backs remain flat. Existing exposed placements and cleared approaches retained. |
| Timber rail / fence caps | **Retained/refined.** Three real guardrails preserve numerical geometry and collision; Winter-local timber replaces their surface material. Existing exposed pond rail top/post caps are retextured. Reusable complete snow rail/fence modules remain exact and exhibited in Zoo. |
| Icicle clusters / varying tips | **Retained geometry, retextured.** Five closed pentagonal families retain correct roots, separated spikes and supported mounting; exposed ledges/hoods/eaves remain dressed. No sheltered lodge sills acquire impossible snow. |
| Frozen plates / fracture pattern / snow boundary | **Remade + new.** New slate-blue angular ice PNG on existing blend floor and opaque submerged backing. Original opacity, traction and shoreline heights retained. Three thin closed `ice_fragment` props add economical plate relief, embedded slightly into the frozen surface. Existing tapered dry corner shelves and snow transition caps retained. |
| Layered mountain horizon | **New Winter placement of retained stock.** Four actual completed Outdoors `boundary_ridge` models behind existing containment. They create skyline depth without a fake background image or new collision. |
| Aurora / stars / calm flakes / blizzard | **Retained exact systems and artwork.** Sky settings, moon, calm/severe weather definitions, particle controls and atmospheric renderer remain unchanged. Actual severe captures retain the intended near-whiteout. |
| Strung warm bulbs / exposed versus sheltered rhythm | **Retained.** All three fitted emissive string families and ordinary owned lights remain; cottage wires/brackets move forward to clear the new hood. No new entity or animation. |
| Bare trees / footprints / signs / benches / utilities | **Omitted by reference evidence.** None is clearly established as a required object in this sheet; no speculative family added. Existing forest stump resting pocket is retained. |
| Full distant village / church | **Remaining composition gap.** The sheet's distant settlement silhouette is larger than the playable three-cottage composition. This pass improves those three buildings and the real mountain layer without expanding into a new settlement or inaccessible church interior. |

The [individual model inventory](model-inventory.json) audits **all 38 Winter GLBs: 10 remade, 14 retextured, 7 retained and 7 new**, with before/after triangle counts, topology, PNG identities and actual placement counts. The [borrowed static inventory](borrowed-model-inventory.json) reviews the full before/current union of **28 shared families**: 21 retained in Winter and seven replaced there while their source files remain byte-identical. The [texture inventory](texture-inventory.json) covers all 14 non-reference Winter PNGs, including four masters and fitted derivatives. Borrowed PNGs are audited with their models and preserved exactly. No existing catalog entry is altered; twelve Winter-local entries are added.

## Construction and technical inspection

New masonry is actual coursed geometry where silhouette/joints matter, with simpler tiled mineral artwork on large wall planes. The fitted stone/wood/metal/amber atlas preserves a 1024² master and native 256² derivative; stone is cropped within one painted block so model courses own the joints. Four raster masters were authored using the built-in ImageGen tool. [Artwork provenance](artwork-provenance.json) records source hashes, briefs, resolution and periodic repair. `tools/props/build_winter_textures.py --check` reproduces exact repaired and Lanczos derivatives from committed art; no runtime or normal builder paints textures.

Snow remains sealed economical geometry with supported bases. Evergreen coats retain every base record; rock crowns query the actual canonical faces. Drift crowns are asymmetric without extra tessellation. New village stock ranges from 24 to 252 triangles, all below 800; existing tree review exceptions remain below 1,500 without raising any budget. Atlas UVs are finite and fitted to 0..1, models are centred in X/Z with Y=0 contact and +Z fronts, and ordinary static emission is restricted to lantern glass.

The first native candidate exposed a floor-relative mounting error: raised lodge window frames and the masonry entrance added the deck rise twice. Final anchors now align with actual sill and doorway heights. The candidate assets/binaries and all twelve first native PNGs remain in the review archive. Initial tree rim winding and one timber wrap offset were corrected at their causes; original failures remain in validation evidence. No assertion tolerance is lowered or check removed to obtain a pass. The old simple hood's scene-coverage expectation now explicitly requires both genuine replacement hoods while preserving its library/Zoo validation.

## Matched native views

All PNGs below are unmodified actual native wgpu/Metal pixels. The clean before copy includes its matching assets, packages and release binaries. The after copy includes final art/packages with matching binaries. [Provenance](provenance.json) pins camera, spawn, package/binary/image hashes and settings. `tools/bench/capture_winter_style.py` freezes twelve High cameras, four matched Low cameras and four severe cameras. High uses full lightmaps/reflections, bloom, 60° FOV and a 640×360 logical / 1280×720 drawable window. Stationary shots capture at 1.5 s; the elevated overview captures the first native presented view. Snow positions can differ with native startup timing.

| View | Before | After |
| --- | --- | --- |
| Square / warm focal rhythm | ![Before square](before/square.png) | ![After square](after/square.png) |
| Lodge / steps / snow support | ![Before lodge](before/lodge.png) | ![After lodge](after/lodge.png) |
| Entrance / framing / dry shelter | ![Before entrance](before/entrance.png) | ![After entrance](after/entrance.png) |
| Cottage / stone / braced hood | ![Before cottage](before/cottage.png) | ![After cottage](after/cottage.png) |
| Evergreen coats / night silhouette | ![Before tree](before/tree.png) | ![After tree](after/tree.png) |
| Forest / stone approach | ![Before forest](before/forest.png) | ![After forest](after/forest.png) |
| Pond / frozen boundary | ![Before pond](before/pond.png) | ![After pond](after/pond.png) |
| Ice / angular plates | ![Before ice](before/ice.png) | ![After ice](after/ice.png) |
| Rail / caps / frozen transition | ![Before rail](before/rail.png) | ![After rail](after/rail.png) |
| Path / broad snow texture | ![Before path](before/path.png) | ![After path](after/path.png) |
| Entire composition | ![Before overview](before/overview.png) | ![After overview](after/overview.png) |
| Warm interior / preserved domestic fittings | ![Before interior](before/interior.png) | ![After interior](after/interior.png) |

Ordinary Low views retain the same cameras with lightmaps/reflections Off. These are useful additional evidence for fitted artwork and geometry; no image exposure correction is applied.

| Low view | Before | After |
| --- | --- | --- |
| Evergreen | ![Before Low tree](before-low/tree.png) | ![After Low tree](after-low/tree.png) |
| Cottage | ![Before Low cottage](before-low/cottage.png) | ![After Low cottage](after-low/cottage.png) |
| Ice | ![Before Low pond](before-low/pond.png) | ![After Low pond](after-low/pond.png) |
| Snow/path | ![Before Low path](before-low/path.png) | ![After Low path](after-low/path.png) |

Severe weather remains a near-whiteout outside. Its matching square/cottage/pond/interior pairs are retained under `before-severe/` and `after-severe/`; they establish preservation, not improved static readability through the storm. Six established native held-control routes pass in **both** calm and severe modes. [Traversal evidence](traversal.json) records controller bounds, exact weather/package identities, completed presentations, shelter accounting and zero particle capacity growth.

## Gains and remaining limits

Masonry and timber construction now carry the reference's village identity, exposed snow has stronger connected silhouettes, broad snow variation survives native downsampling, and the frozen surface reads as fractured blue plates. Stone approaches, warm post fixtures and real skyline ridges add the missing object layers. Warm points remain localized against cool snow/ice/sky. Existing roof, stairs, ramp, pond access and weather remain playable.

The geometry stays more angular and repetitive than the illustrated evergreens. Perimeter containment still reads as an enclosing rocky arena in the elevated overview. The wide roof load is more regular than the sheet's draped accumulation, and ice is darker under actual cool night transport than the isolated concept vignette. The complete distant village/church is omitted as described above. Severe whiteout intentionally hides exterior detail. These are visible limits, not corrected screenshots or claims of exact image reproduction.

Costs, checks and compatibility are in [validation](validation.md) and [performance data](performance.json). Playable before/candidate/after fixtures, native logs/CSVs/settings and intermediate failures live outside `target` in the [retained archive](../../../debug-maps/winter-style-20261007/README.md). Parent integration owns the next branch transition and Art-style queue; this task stops at the standalone pushed Winter revision.
