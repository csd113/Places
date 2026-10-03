# Desktop lighting quality pass

Completed 2026-10-03 on Apple M2 Pro, 12 CPU cores, 16 GiB unified memory. Systemic UV, receiver-normal, GI-cache/reconstruction and adaptive visibility repairs are validated; Lantern Hollow now uses a true baked directional moon. The full repository gate passes. Known finite-resolution and prop-filter limits are documented below.

Scope: the modern desktop engine and compiler, starting at `540d111920cea71a43ac98f3e3ae175ab28a99bd`. The original player, compiler and all seven packages were preserved before changes. No source texture, model texture, roof placement, local light strength or ambient constant was repainted or increased. Lantern Hollow receives the newly supported directional moon. No dependencies or commits were added.

## 1. Prop blotches

Several failures overlapped. Model receivers used geometric normals while the runtime reconstructed light with imported shading normals. The nearest-surface lookup could choose an adjoining face at a hard edge. Smooth-normal gathers could trace below the actual geometric hemisphere, and the diffuse cache could emit a front surface's irradiance through its opaque back face. These are now consistent: imported normals drive lighting response, the geometric normal selects the receiving face and ray separation, and an opaque back face blocks visibility without transmitting its front irradiance.

Atlas UVs addressed `chart_origin + chart_size * local_uv`, although the bake samples both endpoints of an inclusive grid. Runtime UVs now address `chart_origin + 0.5 + (chart_size - 1) * local_uv`. A one-texel chart stays at its centre. Existing separate chart gutters cover the actual single-mip bilinear footprint; chart storage is not a mipmapped texture.

Some apparent blotches are real small-object shadows reconstructed over an excessively large binary sample. The Hollow refrigerator's 2 cm handles and small hinges are real geometry. The baseline direct stage already contains isolated black texels on an otherwise smoothly lit door. Adaptive coverage reduces the footprint of those shadows instead of disabling the blockers. Matched Model Zoo captures show the false door diagonal removed and the hinge blotch substantially reduced, with handle contact shadows retained.

## 2. Jagged diagonal shadows

The original bake evaluated receiver visibility at one position per output texel. The denoiser could then average direct illumination independently within each chart. This both quantized diagonals and made a chart border change reconstruction.

A centre pass now classifies visibility changes. Only edges integrate deterministic stratified receiver footprints: 2×2 at Medium, 4×4 at High. Architectural chart borders compare supported world-space neighbours; a border alone is not a visibility edge. Unchanged direct fields keep their centre sample. Unsupported footprints clamp back to their real receiver. Direct illumination is not denoised.

The 45° numerical control holds a 17×17 receiver grid over a 2 m square fixed across modes (8 intervals/m) and compares against an independent 64×64 area-coverage oracle. This isolates visibility integration from density changes. Across 22 partially covered interior texels, total absolute error is **7.6676893** for one position, **3.2703688** at Medium and **1.6020381** at High: 57.3% and 79.1% reductions. Fully shadowed interiors remain below `1e-6`. Direct ray counts are **289 / 509 / 1169**, respectively; refinement is not charged to every texel.

## 3. Wall-strip discontinuities

Bounce-cache lookup previously required the exact hit triangle. Equivalent coplanar triangles therefore selected different representatives or missing cache neighbours. Cache irradiance now shares a visible oriented plane across triangle boundaries. The hit triangle still supplies its own albedo; geometry, material, interaction and object identities are retained.

GI reconstruction previously stopped at each chart. Its filter now follows the physical architectural surface across chart joins only when kind, geometric and shading normals, bake albedo and visible connectivity agree. Actual corners, dividers and different bake albedos retain their separation. Material IDs and assignments are preserved; compatibility checks bake albedo rather than material IDs. The direct field is re-added unchanged.

Controls compare one 8 m wall against four 2 m vertical strips, horizontal strips and actual vertical wall geometry, including diffuse bounces. The required discrepancy is below `2e-4` at Medium and High.

## 4. Windows and openings

The same triangle-local cache and chart-local reconstruction failures affected separate wall pieces around openings. A further numerical defect was exposed by the window control with a real floor bounce: reconstructed ray hits occasionally rounded just behind their own plane. A cache connectivity segment then intersected its receiving surface at the endpoint and rejected a valid neighbour.

Cache endpoints are now projected onto the hit plane and separated toward its air side with the existing position-rounding bound. Distance ties between grid-equivalent representatives resolve by world coordinates instead of chart iteration order. The rectangular-window control compares four surrounding sections against 22 smaller tiles, with two bounce orders, including points immediately beside the opening and chart joins. It passes the `2e-4` tolerance at both qualities. The known Places Demo pool window is included in the fixed production camera list.

## 5. Corner isolation

Opaque visibility remains watertight, two-sided and based on f64 triangle intersection. Ray separation remains tied to float position precision, not source distance. No broad bias or AO-radius change was introduced. Back-face bounce transport and erroneous cache endpoint rejection were corrected; connectivity checks remain mandatory when cache samples and GI neighbours are shared.

Closed-box directional controls at world scales **0.01, 1 and 1000** require zero interior illumination. Existing thin-wall, divider, water-extinction and probe-isolation controls remain part of the transport suite.

## 6. Apparent diagonal house shadows

The user identified the four houses in Lantern Hollow. Two fixed interior views per house, at Medium and High, are preserved for comparison. Investigation separates physical furniture shadows from triangle-shaped lighting discontinuities.

House 0's fridge door has a false diagonal between original Medium charts **10313 and 10314**, on the front face at `x=-30.47`. Their shared direct samples agree; their raw bounced field differs by **0.0833693743** and filtered field by **0.0473701954** in a linear RGB channel. Across **71 identical physical positions**, the final raw discrepancy is **0.0002478659** (**99.7% reduction**); filtered discrepancy is **0.0129529238** (**72.7% reduction**). The remaining small prop-local filter difference is acknowledged in section 19. The corresponding clean quad cache control previously differed by 0.01875 when only the hit triangle changed; it now agrees within `1e-5`. [Full samples and chart coordinates](lighting-quality-pass/fridge-final-diagonal-comparison.json).

The selected black door samples have positively identified physical casters: **all 30 queried rays hit the fridge's own handles or hinges**, **0.77–4.04 cm** from their receivers. Triangle **12667** is the middle hinge, **12630** the upper handle, **12650/12651** the upper hinge and **12678** the lower hinge. Owner `model:core/props/models/fridge.glb`, world corners, origin, direction, distance and triangle ID are preserved in the [exact selected hits](lighting-quality-pass/house-selected-caster-evidence.json). Those small legitimate shadows were overrepresented by binary texel-centre visibility; adaptive footprint coverage corrects their reconstruction. The separate full-door diagonal originates in indirect transport and has no roof caster.

## 7. Roof involvement

The final production investigation tests **1,452 valid visible-air floor rays**, **363 per house**, directed upward and toward its living/bedroom fixtures. There are **zero roof hits**. First blockers include **476 ceilings, 268 partitions, 48 tables, 32 beds** and other legitimate furniture; **500 segments are unobstructed**. These rays are separate from the 30 selected refrigerator rays. The [summary](lighting-quality-pass/house-caster-summary.json) and [complete ray records](lighting-quality-pass/house-caster-rays.json) are preserved.

The preliminary investigation included forty upward roof hits from origins buried inside partition footprints, where the ceiling intentionally excludes hidden geometry. Those origins were excluded from the final visible-air receiver set. They do not demonstrate a leak into a visible interior.

The diagnostic building contains four walls, floor, ceiling and a pitched roof split into diagonal triangles. The focused ceiling/roof regression also proves that adding the roof leaves its local-light/bounce result unchanged, and upward interior rays hit the ceiling first. All Hollow roofs and gable ceilings remain present. The observed defects did not justify changing roof geometry or caster classification.

## 8. Global illuminator architecture

`LevelDef.global_illuminators` is an optional bounded list of at most eight directional sources in format 3. Each exposes `id`, `kind`, `direction`, `color`, `intensity`, `enabled`, `cast_shadows`, `bake` and `angular_size_degrees`. Authoring direction is light travel from source toward world; the prepared source stores the opposite normalized incoming direction. IDs must be unique and nonempty; finite vector, color, intensity and angular-size domains are validated before preparation. Normalization uses f64 so every valid finite f32 direction can normalize safely.

Sources have no position, range or inverse-square attenuation. Infinite shadow rays use the same opaque/transmissive scene as local light. Angular diameter uses deterministic equal-area disk directions. Direct moon energy and diffuse transport are baked into the base HDR atlas and moving-object probe field. Gameplay Low stores its occluded cosine response in static vertex colors using infinite box/slab visibility. Open ceilings no longer create invisible Low occluders.

Compiled lighting records advance from version 1 to 2. Cache version is 13, transport revision 11 and geometry revision 3. Current packages must be rebuilt. Invalid compiled global-light data is rejected. There is no new per-frame global-light loop, shadow texture or shadow pass.

## 9. Lantern Hollow moon

The generator authors `moon`, direction **[-0.36, -0.8, -0.48]**, RGB **[0.85, 0.9, 1.0]**, intensity **0.12**, full angular diameter **0.5°**, enabled with shadows and bake participation. This is a cool near-neutral low source, with warm local lamps retaining their roles. `sky.ambient` remains **0.22**. The star texture has no visible moon requiring alignment; no new sky imagery was added.

The directional source and existing diffuse sky contribution remain separate. Closed surfaces block direct moon energy normally. Fixed street, house and pond captures at Low, Medium and High verify readable exterior geometry, warm lamp contrast and locally lit interiors. Low retains its coarser, brighter legacy vertex-light character; it is not presented as equivalent to the HDR bake.

## 10. Engine, compiler, format and tooling changes

Contained changes cover endpoint atlas UVs, quality-related model density, imported/transformed normals, reflected static glTF winding, oriented receiver selection, cache connectivity, GI filtering and adaptive direct coverage. No renderer architecture or material shader was replaced. Negative determinant glTF node transforms swap triangle winding while preserving inverse-transpose normal transforms.

The compiler logs receiver setup, direct visibility, indirect transport, probes, reconstruction, fill and atlas assembly timings. Geometry/chart preparation is separately timed for props and architecture; these are combined planning stages, not falsely labelled pure chart-allocation time.

`tools/levels/build_lighting_quality.py` emits deterministic geometry using existing catalog assets and 20 fixed diagnostic views. It includes continuous/subdivided walls, equivalent window surrounds, door/inset openings, concave and convex corners, thin/thick enclosed rooms, connected/stacked rooms, ceiling with pitched roof, rotated cubes, cylindrical/curved props, scaled near-wall props, representative production models, an isolated curved model, a thin diagonal caster and grazing surfaces with slightly different normals.

`PLACES_LIGHTING_DUMP_DIR` exports direct, bounced, indirect, filtered, filled, global direct and normal stages, HDR receiver RGB and chart metadata. `PLACES_LIGHTING_LOCAL` isolates a local emitter. `PLACES_LIGHTING_RAYS` accepts bounded validated requested rays and reports the first opaque triangle, owner, world corners, normal and distance. Separate AO and runtime shadow-map stages do not exist in this engine.

`capture_lighting_quality.py` preserves full settings, cameras, fixed time, binary/package/image hashes and native PNG dimensions. It checks package stability across the native run. Earlier captures predate the package-hash field; the index explicitly identifies hashes subsequently collected from preserved unchanged packages. It rejects a requested-level failure even when the player falls back to the demo. `inspect_lighting_dump.py` projects triangles and trapezoids through the actual piecewise affine UV mapping, isolates world faces, exposes chart IDs/density and reconstructs old UV sampling for controls. NumPy/Pillow are optional analysis tools, not game dependencies.

The chart inspector's `--audit` checks finite/nondegenerate geometry, unit normals, atlas bounds and disjoint padded reservations. Original and final Hollow Medium both pass **all 230,117 charts with zero errors**. The deliberate malformed control detects overlap, degeneracy, nonunit normals and zero width. One-sample axes and extreme elongation remain reported budget/pathology diagnostics, rather than automatically being called corrupt. `inspect_metal_trace.py` unions overlapping active GPU channels and weights resource-allocation states by duration.

The five representative production meshes—fridge, water cooler, boulder, stump and rubber duck—have no duplicate faces, zero-area UV triangles, inverted triangles, nonmanifold edges or geometry-audit errors. The fridge has **108 triangles and nine closed components**. All five contain no authored normals: imported-normal repairs are protected by transformed synthetic controls and are not falsely claimed as this fridge diagonal's cause. [Geometry audit](lighting-quality-pass/representative-model-geometry-audit.json).

The subsystem classification is supported by controls rather than appearance alone:

| Subsystem | Finding and evidence |
| --- | --- |
| Map geometry and surface segmentation | Equivalent clean wall/opening controls diverged before the cache/reconstruction repairs; no manual wall brightness correction is needed. |
| Model geometry / caster classification | Small refrigerator handles/hinges are legitimate opaque blockers; source meshes remain intact. Reflected glTF winding is corrected and tested. |
| Triangle splitting / interpolation | Exact-triangle GI cache identity creates a false diagonal; coplanar oriented-plane cache controls protect the repair. |
| UV generation / atlas sampling | Bake endpoints and old runtime coordinates disagreed. The endpoint-centre regression covers 1×1 and larger charts. |
| Chart generation / density | Separate charts remain separate; density follows quality. Metadata exposes actual world spans and sample counts. |
| Packing / padding / dilation | Shared-page planning and isolated-gutter tests check disjoint footprints and neighbouring colors; no unsafe wider filter or atlas mip was added. |
| Direct visibility | Binary receiver-centre visibility is quantized; 45° oracle establishes the coverage improvement. |
| Indirect light / reconstruction | Triangle-local lookup, rounded connectivity endpoints, back-face emission and chart-local filtering caused discontinuities. Controlled bounced wall/window tests protect each boundary. |
| Ray precision / bias / BVH | Existing f64 two-sided intersection and scale-aware separation are preserved; scaled enclosure, self-intersection and ceiling controls test isolation. No evidence justified a larger bias. |
| Normals | Bake and runtime now agree on imported shading normals; geometric normals retain visibility ownership. Hard edges and reflected nonuniform transforms are tested. |
| Tangent space | Runtime normal-map tangent/handedness reconstruction is retained. The baker does not sample tangent-space normal textures. These do not explain a discontinuity already present in linear bake-stage exports. |
| AO / runtime shadow maps | Neither is a separate stage in this baked desktop path; there is no AO radius, PCF kernel, shadow cascade or runtime shadow-map allocation to repair. |
| Vertex lighting | Low receives statically prepared directional cosine/visibility, with its existing coarser occluder contract. |
| Roof / ceiling | Ceiling shielding is tested; all 1,452 final visible-air house rays have zero roof blockers. |
| Environment / global sources | Existing diffuse sky radiance is unchanged. New directional moonlight has independent infinite visibility and no range falloff. |

## 11. Deterministic captures

The [capture index](lighting-quality-pass/capture-index.json) records **169 valid native Metal images**: **39 production before + 39 after**, **48 equal-source local diagnostic A/B images**, **40 directional diagnostic Medium/High images**, and **three Low controls**. Exact cameras, settings, fixed time, image/binary/package hashes and source paths are recorded. **39 selected images and native logs** are stored beside this report. No screenshot was painted, cropped, exposure-adjusted or resampled.

Saved settings request a 1920×1080 window, FOV 60°, VSync off, reflections off and time zero. macOS fits the window to the work area and uses a Retina backing surface, producing **3024×1676 native PNGs**, identical across A/B. Native logs must commit the requested map. The local-control A/B disables directional bake participation in the same source on both compilers; the production Hollow comparison intentionally includes the moon. Failed requested-level loads, obsolete empty Model Zoo cameras, preliminary concurrent-load profiling and stale intermediate captures are excluded.

Visual QC covers the pool/window, wide pool, corridor and home room; Model Zoo fridge and curved props; street, pond and all four Hollow houses; all 20 High diagnostic views and selected Medium comparisons. Enclosed, unlit diagnostic rooms stay black. Slightly different normals remain actual surface discontinuities. The fixture cameras show technical controls, not a finished artistic map.

| Evidence | Before | After |
| --- | --- | --- |
| Pool window, Medium | [Native](lighting-quality-pass/production-before-pool_window-medium.png) | [Native](lighting-quality-pass/production-after-pool_window-medium.png) |
| Pool window, High | [Native](lighting-quality-pass/production-before-pool_window-high.png) | [Native](lighting-quality-pass/production-after-pool_window-high.png) |
| Zoo fridge, Medium | [Native](lighting-quality-pass/production-before-zoo_fridge-medium.png) | [Native](lighting-quality-pass/production-after-zoo_fridge-medium.png) |
| Zoo fridge, High | [Native](lighting-quality-pass/production-before-zoo_fridge-high.png) | [Native](lighting-quality-pass/production-after-zoo_fridge-high.png) |
| Curved props, High | [Native](lighting-quality-pass/production-before-zoo_curved-high.png) | [Native](lighting-quality-pass/production-after-zoo_curved-high.png) |
| Corridor, High | [Native](lighting-quality-pass/production-before-corridor-high.png) | [Native](lighting-quality-pass/production-after-corridor-high.png) |
| Hollow house 0 gable, Medium | [Native](lighting-quality-pass/production-before-hollow_house0_gable-medium.png) | [Native](lighting-quality-pass/production-after-hollow_house0_gable-medium.png) |
| Hollow house 0 gable, High | [Native](lighting-quality-pass/production-before-hollow_house0_gable-high.png) | [Native](lighting-quality-pass/production-after-hollow_house0_gable-high.png) |
| Hollow house 0 floor, High | [Native](lighting-quality-pass/production-before-hollow_house0_floor-high.png) | [Native](lighting-quality-pass/production-after-hollow_house0_floor-high.png) |
| Hollow street, Low | [Native](lighting-quality-pass/production-before-hollow_street-low.png) | [Native](lighting-quality-pass/production-after-hollow_street-low.png) |
| Hollow street, Medium | [Native](lighting-quality-pass/production-before-hollow_street-medium.png) | [Native](lighting-quality-pass/production-after-hollow_street-medium.png) |
| Hollow street, High | [Native](lighting-quality-pass/production-before-hollow_street-high.png) | [Native](lighting-quality-pass/production-after-hollow_street-high.png) |
| Hollow pond, High | [Native](lighting-quality-pass/production-before-hollow_pond-high.png) | [Native](lighting-quality-pass/production-after-hollow_pond-high.png) |
| Vertical strips, Medium, equal local source | [Native](lighting-quality-pass/local-control-before-vertical_strips-medium.png) | [Native](lighting-quality-pass/local-control-after-vertical_strips-medium.png) |
| Vertical strips, High, equal local source | [Native](lighting-quality-pass/local-control-before-vertical_strips-high.png) | [Native](lighting-quality-pass/local-control-after-vertical_strips-high.png) |
| Window pieces, Medium, equal local source | [Native](lighting-quality-pass/local-control-before-window_separate_pieces-medium.png) | [Native](lighting-quality-pass/local-control-after-window_separate_pieces-medium.png) |
| Window pieces, High, equal local source | [Native](lighting-quality-pass/local-control-before-window_separate_pieces-high.png) | [Native](lighting-quality-pass/local-control-after-window_separate_pieces-high.png) |
| Ceiling/pitched roof, High, equal local source | [Native](lighting-quality-pass/local-control-before-ceiling_pitched_roof-high.png) | [Native](lighting-quality-pass/local-control-after-ceiling_pitched_roof-high.png) |

The final directional caster views are [Medium](lighting-quality-pass/directional-control-after-diagonal_thin_caster-medium.png) and [High](lighting-quality-pass/directional-control-after-diagonal_thin_caster-high.png). The [thin enclosed room](lighting-quality-pass/directional-control-after-thin_closed_corners-high.png) stays black. The independent area oracle quantifies diagonal coverage rather than relying only on a dim nighttime image.

## 12. World-space texel density

| Receiver class | Before Medium / High | After Medium / High |
| --- | --- | --- |
| Architecture | 12 / 16 texels/m nominal | 12 / 16 |
| Small opaque model | 8 / 8 | 6 / 8 |
| Large opaque model, span over 3 m | 1 / 1 | 1.5 / 2 |
| Cutout model | 1 / 1 | 1 / 1 |
| Gameplay Low | Static vertex lighting | Static vertex lighting |

Model densities now derive from the architecture quality rather than remaining fixed across qualities. Actual inclusive endpoint density is `(axis_texels - 1) / physical_axis_span`; rounded chart dimensions and tiny one-sample axes are reported in metadata. The policy is constrained by the existing eight-page budget, including real Hollow foliage/structure fragmentation. It does not allocate arbitrary per-map brightness or density overrides.

Measured examples: Hollow's 9 m architectural floor axis has **108 samples = 11.888889 intervals/m**, unchanged at Medium. Its two front fridge-door charts change from **6×8 / 8×5** to **4×6 / 6×4** at Medium. Measured axis densities change from **[7.575759, 7.907208] / [7.907208, 6.779662]** to **[4.545456, 5.648006] / [5.648006, 5.084746]**. Better visibility/cache reconstruction permits smaller Medium model charts while large surfaces gain density. The equal-source diagnostic's median resolved floor-axis density is **11.854167 / 15.854167** at Medium/High; wall medians are **11.5 / 15.5**. [Full quantiles, representative charts and collapsed-axis counts](lighting-quality-pass/texel-density-measurements.json).

## 13. Sample counts

Medium retains two emitter taps per axis, one diffuse bounce and 32 gather directions; High retains three taps, two bounces and 64 directions. A point emitter has one source position; line/rectangle taps scale with their shape. A global angular disk uses up to taps² directions. Adaptive receiver coverage adds four or sixteen deterministic positions only at detected visibility edges. Bounce sequences remain chart-independent and deterministic. Gameplay Low has no atlas solve.

The isolated 45° coverage control uses 289 / 509 / 1169 direct rays. Compiler stage measurements include real visibility work; no unmeasured map-wide ray total is inferred from nominal taps.

## 14. Offline bake cost

Clean offline measurements use **12 bounded bake workers**, sequential processes, forced fresh output, no diagnostic stage exports and no concurrent native/test jobs. Three repeated alternating A/B builds use the identical local-light fixture with moon bake participation disabled. Observation-only timers in the original compiler are verified to leave its first Medium/High packages **byte-identical to the original preserved compiler output**. Values below are medians of three runs.

| Full fixture compilation | Before, s | After, s | Change |
| --- | --- | --- | --- |
| Medium | 2.920 | 2.630 | -9.9% |
| High | 12.752 | 12.462 | -2.3% |

| Fixture stage, ms | Medium before → after | High before → after |
| --- | --- | --- |
| Legacy/static vertex preparation | 4.05 → 4.05 | 4.10 → 3.99 |
| Prop geometry and charts | 8.71 → 8.57 | 8.53 → 8.22 |
| Architecture and charts | 4.00 → 4.02 | 3.13 → 3.18 |
| Receiver setup | 150.47 → 91.33 | 239.01 → 154.36 |
| Direct illumination + visibility | 68.50 → 106.18 | 199.75 → 379.89 |
| Indirect transport | 1784.73 → 1542.34 | 11059.78 → 10659.78 |
| Moving-object probes | 33.91 → 31.71 | 38.52 → 36.68 |
| GI reconstruction | 322.49 → 331.58 | 545.14 → 581.92 |
| Baseline fill | 78.44 → 84.47 | 134.17 → 139.84 |
| Atlas assembly | 8.09 → 6.36 | 9.23 → 9.04 |

A clean single production **Hollow High** build changes **236.69 → 305.56 seconds (+29.1%)**. This includes the new moon and increased large-model density; it is not the equal-source control. Its direct/visibility stage changes **1.80 → 25.87 s**, indirect **214.84 → 246.71 s**, prop geometry/chart planning **7.62 → 20.01 s**, architecture/chart planning **270.23 → 272.51 ms**, and atlas assembly **80.58 → 91.33 ms**.

Direct lighting and visibility are evaluated together and reported as a combined stage. Geometry/chart preparation also includes planning; it is not falsely labelled pure allocation. Stage medians need not sum exactly to overall medians, which additionally include scene preparation, validation, navigation and package serialization. The extra production cost is offline. [All individual samples and stage records](lighting-quality-pass/offline-samples.json), build logs and original/final compiler hashes are preserved. Full seven-map rebuild timings in the rebuild records include multi-variant/switchable/reflection preparation and are not used as isolated bake A/B benchmarks.

## 15. Runtime CPU/GPU performance

CPU comparisons use three alternating A/B runs per camera/quality, **600 discarded warm-up frames and 1,200 recorded frames per run**: 3,600 recorded frames per side/view. Settings, resolution, fixed camera/time, binaries and package hashes are controlled. No bake/test/profiling job runs concurrently. The first 120-warm-up series showed startup submission stalls in the first 300 recorded Zoo frames. That series is preserved separately and prompted the longer warm-up for every view.

The following values are medians of three per-run medians; p95 values are medians of the three per-run p95s. CPU `frame_ms` measures update + submission with no swap. `render_ms` is CPU submission, not GPU execution.

| Camera / quality | CPU frame median, ms before → after | CPU frame p95, ms before → after | Update median, ms before → after | Submission median, ms before → after |
| --- | --- | --- | --- | --- |
| hollow street-high | 2.152 → 2.146 | 2.249 → 2.229 | 1.510 → 1.500 | 0.641 → 0.645 |
| hollow street-medium | 2.166 → 2.140 | 2.248 → 2.234 | 1.498 → 1.467 | 0.668 → 0.668 |
| pool window-high | 2.023 → 2.061 | 2.502 → 2.476 | 1.419 → 1.421 | 0.565 → 0.617 |
| pool window-medium | 1.629 → 1.627 | 1.705 → 1.700 | 1.225 → 1.220 | 0.404 → 0.403 |
| zoo fridge-high | 1.362 → 1.364 | 1.822 → 1.462 | 1.090 → 1.091 | 0.268 → 0.270 |
| zoo fridge-medium | 1.338 → 1.325 | 1.408 → 1.379 | 1.108 → 1.099 | 0.229 → 0.226 |

| Hollow GPU, ms | Device occupied work / frame, before → after | Main scene span mean, before → after | Main scene span p95, before → after |
| --- | --- | --- | --- |
| Medium | 1.771 → 1.767 | 1.403 → 1.395 | 1.626 → 1.594 |
| High | 2.234 → 2.236 | 1.716 → 1.713 | 1.730 → 1.727 |

GPU results come from native **Metal System Trace**, eight-second recordings attached after successful Hollow load and a two-second settle; the stable trace window is seconds 1–7. Overlapping active hardware channels are unioned, then divided by scene encoder count. This is device occupied work per rendered frame, not display FPS or end-to-end latency. Main-scene spans are reported separately. One trace per side/quality does not support interpreting sub-percent changes as improvements or regressions.

Draw calls, reflection pass counts, VBO bytes and index bytes are identical across each matched CPU camera (Hollow 761 draws, pool window 274, Zoo fridge 25; reflections zero). No new dynamic global-light loop or shadow pass is added. Global energy is already in the existing atlas, probes or Low static vertices. These results establish the measured fixed-camera cost on this M2 Pro, not a guarantee for every GPU/camera. [Raw warmed CPU samples](lighting-quality-pass/cpu-warmed-samples.json), per-frame CSVs and [GPU summaries](lighting-quality-pass/gpu-samples.json) are preserved.

## 16. Memory

This M2 Pro has **16 GiB unified memory**, not separate dedicated VRAM. Each resident 1024² atlas page contains two RGBA16F planes: **16 MiB per layer set**. Low allocates no atlas. No new shadow maps are allocated. Compiled global records are small static CPU data; Medium/High shaders gain no source loop.

| Package | Medium atlas MiB, before → after | High atlas MiB, before → after |
| --- | --- | --- |
| lantern_hollow | 96 → 96 | 112 → 128 |
| model_zoo | 96 → 96 | 112 → 128 |
| movement_test | 48 → 48 | 80 → 80 |
| places_demo | 224 → 192 | 256 → 256 |
| geometry_intentional | 16 → 16 | 16 → 16 |
| home_showcase | 16 → 32 | 16 → 32 |
| level0_pit | 48 → 64 | 64 → 80 |

Places Demo includes its base and switchable layer set; all other rows use base-only atlases. Hollow High changes **seven → eight pages**, adding **16 MiB**, while Medium remains six pages. Model Zoo High adds one page. Home and Pit each add a page; Demo Medium saves a page in each layer set. These costs are not concealed as a universal memory improvement. [Package bytes, hashes, charts, sample totals and atlas resources](lighting-quality-pass/package-resource-comparison.json) include all seven packages.

Native Metal allocation covers all device resources rather than the atlas alone. The duration-weighted median and stable-window peak are:

| Hollow quality | Median allocation MiB, before → after | Peak allocation MiB, before → after |
| --- | --- | --- |
| Medium | 281.06 → 281.06 | 286.84 → 286.84 |
| High | 388.20 → 404.75 | 395.08 → 406.72 |

The approximately 16.5 MiB High median increase is consistent with its extra 16 MiB atlas page plus transient resource variation. Medium allocation is unchanged. Device allocation is not process RSS or physical dedicated VRAM. CPU geometry-buffer sizes remain identical in the matched views. No separate runtime shadow allocation is introduced.

## 17. Regression coverage

Fourteen quality controls cover coplanar hit-triangle cache identity, diagonal shadow coverage, constant infinite direction/no falloff/determinism, scaled closed-room isolation, ceiling shielding from pitched roofs, imported normals, oriented hard faces, direct continuity, chart-border coverage, actual vertical/horizontal wall subdivisions, window surrounds with floor bounce, tilted-normal self-bounce and opaque-backface probe emission. Three directional authoring/compiled-record/Low tests and a reflected nonuniform glTF transform test accompany them.

Existing atlas endpoint/isolated-gutter controls and the real gable mesh-to-chart UV test protect the new sampling contract. The transport suite passes 68 tests; the 14 quality controls pass after the endpoint fix. The full repository gate passes; results are recorded below.

## 18. Validation and package currency

The authoritative **`sh tools/verify.sh` exits 0**, with the current source, compiler and packages. The [complete log](lighting-quality-pass/verify-final.log), [exit result](lighting-quality-pass/verify-final-result.json) and [validation summary](lighting-quality-pass/validation-summary.json) are preserved.

| Validation | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed in full gate |
| `cargo check --workspace --all-targets --all-features` | Passed, 4.13 s final run |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` | Passed, 7.38 s final gate |
| `cargo test --workspace --all-features` | 1,931 library tests passed, 21 intentionally ignored; three integration tests passed; zero failures |
| Supplemental showcase/static-prop checks | Nine / 19 passed; zero failures |
| Ignored bundled Medium/High atlas preflight | Passed |
| Ignored native GPU Low/resource-toggle tests | Both passed |
| Python suites including compiler builds and native bootstrap | All 182 tests passed |
| Asset validation and complete asset audit | Passed: 247 assets, 134 models, four themes; inventory has 35 maps, 136 embedded PNGs and 159 standalone PNGs; no warnings/errors |
| Maintained asset/entity/generator checks | Passed, including Hollow and new lighting fixture |
| Four shipped package currency/decode checks | Passed in gate |
| Three local development package currency/decode checks | Passed separately |
| New Python tools: `python3 -m py_compile` | Passed after final edits |
| `git diff --check` | Passed after report/evidence finalization |

All seven packages are rebuilt with Low/off, Medium and High/full variants and matching source/asset fingerprints: **places_demo, model_zoo, lantern_hollow, movement_test, geometry_intentional, home_showcase, level0_pit**. The last three packages are intentionally ignored local development artifacts. [Exact currency results](lighting-quality-pass/package-currency.json) and [local decode results](lighting-quality-pass/local-package-validation.json).

Earlier runs interrupted for stale geometry-revision and Hollow page-count assertions are not counted as passes. Those assertions now reflect revision 3 and the validated eight-page Hollow High plan. The final complete gate has no failed tests.

## 19. Remaining limitations

Finite density and diffuse gather counts remain deliberate stylized budgets. Tiny handle/hinge contact shadows are integrated within receiver footprints, not ray-traced per pixel. Hollow's fridge-door filtered chart discrepancy remains **0.0129529** linear RGB maximum, reduced from **0.0473702**; prop reconstruction stays local to its chart, whereas architectural joins share compatible physical-surface reconstruction. The primary false diagonal field is reduced by 99.7%, not described as mathematically zero.

Cutout models retain their existing inexpensive caster contract. Directional sources are baked static illumination; `bake:false` sources are inactive, and this work does not add animated day/night shadows. Low keeps coarse static vertex/box visibility and has no Medium/High diffuse probe solution. It can look brighter and less detailed than the HDR bake. Tangent-space normal textures remain a runtime response detail rather than a bake input.

The eight-page budget remains binding; several production High packages are at that limit. A higher large-model density was evaluated and rejected because Hollow could not fit. The final coherent policy fits all seven maps. Native runtime results measure warmed fixed-camera rendering on this host, not all-camera gameplay, other GPUs or a guarantee against every authored micro-gap. No source imagery, local-light strength, ambient constant or roof geometry was changed to mask errors.


## Changed files

Engine, compiler and regressions:

- `src/compiler.rs`
- `src/gltf.rs`
- `src/gltf/tests.rs`
- `src/level.rs`
- `src/level/tests.rs`
- `src/lighting.rs`
- `src/lighting/bake.rs`
- `src/lighting/lightmap/cache.rs`
- `src/lighting/lightmap/mod.rs`
- `src/lighting/lightmap/plan.rs`
- `src/lighting/lightmap/tests.rs`
- `src/lighting/transport.rs`
- `src/lighting/transport/tests.rs`
- `src/lighting/visibility.rs`
- `src/lighting_audit_cases.rs`
- `src/loader.rs`
- `src/loader/tests.rs`
- `src/render/common/api.rs`
- `src/render/common/architecture.rs`
- `src/render/common/geometry.rs`
- `src/render/common/light_transport.rs`
- `src/render/common/mod.rs`
- `src/render/common/props.rs`
- `src/static_prop_lighting_tests.rs`
- `src/lighting/directional.rs`
- `src/lighting/directional/tests.rs`
- `src/lighting/transport/coverage.rs`
- `src/lighting/transport/diagnostics.rs`
- `src/lighting/transport/filter.rs`
- `src/lighting/transport/tests/quality.rs`

Map and generated packages:

- `assets/levels/lantern_hollow.json`
- `assets/levels/lantern_hollow.placesmap`
- `assets/levels/model_zoo.placesmap`
- `assets/levels/movement_test.placesmap`
- `assets/levels/places_demo.placesmap`

Developer tools:

- `tools/levels/build_lantern_hollow.py`
- `tools/verify.sh`
- `tools/bench/capture_lighting_quality.py`
- `tools/bench/inspect_lighting_dump.py`
- `tools/bench/inspect_metal_trace.py`
- `tools/bench/lighting_quality_views.json`
- `tools/levels/build_lighting_quality.py`

Fixture:

- `tests/fixtures/levels/lighting_quality.json`

Documentation:

- `docs/MAP_AUTHORING_GUIDE.md`
- `docs/PACKAGE_FORMAT.md`
- `docs/VERIFICATION.md`
- `docs/reports/lighting-quality-pass.md`

Local rebuilt packages: `levels/geometry_intentional.placesmap`, `levels/home_showcase.placesmap`, `levels/level0_pit.placesmap` (ignored development artifacts). The `docs/reports/lighting-quality-pass/` evidence directory contains 39 native images with matching logs, the complete capture index, causal measurements, audits, package resource/currency/decode results and validation/performance records. The index enumerates the image files individually. No commit was created.
