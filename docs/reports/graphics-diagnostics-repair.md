# Graphics and diagnostics repair — 2026-10-12

Five reported problems were traced through the existing renderer and asset pipeline. No shipped map source or visual PNG was redesigned. Arcade and Winter are the native acceptance targets; the retired Demo and Model Zoo are historical sources only.

## Causes and corrections

**Fixtures.** Luminous vertices discarded source colour, added a brightness floor and clipped emission at one. They now carry the source’s linear colour × authored emission, defaulting to brightness. The existing fitted PNG still shapes the diffuser. Fixture asset IDs also reach the existing emission-animation uniforms; independent face slots and switch × animation factors keep an off fixture dark through flicker.

Fluorescent housings now use a non-emissive metal definition referencing the existing metal PNG and normal map. Ordinary Static vertex lighting and Full receiver charts handle illumination; the body never enters the emission pass. It does not introduce a reflection-probe dependency. Making the body genuinely opaque exposed another defect: source taps sat on the housing’s outer frame at its mounting plane. The source now matches the recessed diffuser aperture, with equal-area midpoint samples instead of finite weight on zero-area edges. Real housing shadows remain. Turned borders and UVs preserve the same 2:1 sheet fit. Geometry revision 11, the lighting fingerprint and solver revision 19 invalidate obsolete cached results.

**Prop warnings.** The ten Arcade warnings concerned four 1024px cloths and six 512px gifts, already approved in the asset specification. Runtime still assumed every model had a 256px budget, while asset tests used broader name-prefix exceptions. Both now use the same exact reviewed file paths. Unreviewed assets, oversized edges and triangle violations still warn. Catalog/model bounds agreed within 0.000005 m; neither scale nor geometry needed correction.

**Medium textures.** Ordinary 256px props previously uploaded unchanged on both Medium and High. Medium now uses 384px surfaces/fixture faces/decals, 192px props/masks and 768px skies. Proportional fitting replaces integer-only division, which would collapse 256→192 to 128. Sky and decal fitting now follows the existing linear-light, alpha-weighted colour filter, avoiding darkened reduced sheets and hidden-colour bleed. High retains native pixels. Filtering and Static/Full lighting remain independently selected.

**Blizzards.** World extinction and a severity-only sky blend approached different colours, leaving silhouettes against the sky. Sky, opaque/transparent surfaces, models, characters, decals and emission now converge coherently. The squared-exponential curve has a smooth optical-depth 2–3 tail ending at exact zero; transparent steam also loses distant contrast. Complete exterior occlusion is at 50/21.43/7.5 m for Winter’s mild/moderate/severe presets. Sheltered sightline intervals, nearby lights, snowfall seeds and weather controls are retained.

**GPU and RAM overlay.** The previous GPU percentage sampler only searched Linux interfaces. This M2 Pro supports Metal pass-boundary timestamp queries, although arbitrary encoder timestamps are unavailable. The device requests its supported feature; three persistent slots retain queries through GPU completion, resolution and asynchronous readback. The meter sums actual render-pass durations in milliseconds, excluding CPU/upload/presentation waits and screenshot rerenders. Invalid samples have explicit reasons and may recover; unsupported adapters report their actual limitation.

The visible overlay also reads the published whole-device AGX utilization counter and current-process RSS every 500 ms through cached public APIs. GPU device percent includes other applications; game render milliseconds are separate. RAM is resident MiB, without adding driver-owned unified-memory allocations. No helper, subprocess, permission or new dependency is needed for those counters.

## Executed verification

Formatting, strict debug/release workspace/all-target/all-feature Clippy, default Rust tests (2,245 library plus eight integration tests), all-feature Rust tests (2,251 library plus eight integration tests), and the final release build passed. Eighteen pre-existing GPU tests remain explicitly ignored. Asset validation covered 569 entries with zero warnings; 346 props stayed within the existing 106 MiB decoded budget. The source Python suite reported 349 tests with four native classes skipped; four selected bounded native smoke checks subsequently passed. No default Rust test or CI job bakes a shipped map.

Native wgpu/Metal checks used Apple M2 Pro at 1280 × 720. The tiny fixture demonstrated emission 0.25/1/3, the turned aperture, pink source colour, visible flicker, and a textured metal body with a black off diffuser. The middle face’s mean RGB changed from [230.19, 94.74, 159.63] to [168.75, 67.26, 115.79] during the dip. Off images remained identical through flicker. Environmental illumination stays baked. The unchanged water-lighting control measured 0.730 luminance through water and zero through its opaque control; lighting thresholds were retained.

Arcade exercised all six Low/Medium/High texture × Static/Full lighting combinations and live Low/Medium/High filtering, including Low textures with Full lighting. Base world texture bindings occupied 38.88 / 11.38 / 8.88 MiB on High/Medium/Low; this excludes lightmap storage. Returning to High reused native texture identities without reupload. The ten previously captured prop-budget warnings became zero; invalid-budget regressions still reject unreviewed oversized sheets and excessive triangles.

Four scripted E edges aimed at the actual bathroom switch produced on/off/on/off/on. Repeated on and off images were pixel-identical. Full-off → Static-off → Full-off returned the exact same Full-off image; on/off mean RGB differences were [104.81, 107.63, 103.21]. The Cosmic circuit also switched and returned. Native counters confirmed planar reflections in Full and none in Static. Resize/minimize/restore and live setting changes produced no GPU validation or timestamp-data rejection.

Winter’s matched Static views confirmed the distant skyline converges with the fog at strengths 0.15/0.35/1. Full sheltered severe and calm images were pixel-identical, and a nearby lantern remained warm in severe weather. Physical preset/toggle/timer inputs completed the authored cycle: the trace recorded 0.45 during the timed storm, zero during its calm dwell and zero after stopping. Capacity remained 1,400 particles / 5,600 vertices with zero growth. The nearby-lantern witness is a native capture; its occluded normal surface produced no timing sample, and it is not performance evidence.

The isolated overlay used ordinary wall time with the compiler idle and no GPU-completion drain. Its final 240 CPU records contained 157 distinct hardware samples: **7.72 ms median / 9.38 ms p95** summed GPU render-pass time. The retained meter image shows **GPU device 91%, GPU render 7.59 ms and Game RAM RSS 515 MiB**. An independent first driver sample returned 98%; resident bytes matched `ps` exactly at the end of the probe. A 200-read cached public API pair measured 26.19 µs median / 44 µs p95 including Python FFI. Rapid burst utilization reads are not a utilization average. No CPU duration is presented as GPU time, and no performance improvement over the old renderer is claimed.

## Packages and handoff

Only Arcade and Winter were production acceptance builds. After the final colour-filter correction changed the executable identity, those two were refreshed and passed `verify --require-current` and integrity validation. Other six active packages remained byte-identical; they need normal recompilation after their queued edits. No archived map was built or restored to discovery.

| Final ignored package | Compressed bytes | SHA-256 |
| --- | ---: | --- |
| Arcade | 44,274,305 | `0b54fe844ec859f8613aff1f15e2c8796b34cce44b8cec72ce35c5a5732d037f` |
| Winter | 42,585,232 | `2fc8bec49cb56b9e0b8afcd91205ed525cff5779b59400f1fcbbde6d88b27aaa` |

Final player SHA-256: `c5856f45831b9dc16455795c287678d52a0e458f02db6a5e8488acd0df9c9bc7`; compiler: `6103b1563ed7e6a7a19003ac56961483c2b90e25649fac5cdf07238ef150c5db`. Camera/settings inputs are in the three `graphics-*-manifest.json` files under `tests/fixtures/native`; the small source is `tests/fixtures/levels/graphics_fixture_contract.json`. `local/graphics-diagnostics/verification.json` retains compact receipts, exact capture recipes, package identities and numerical evidence outside `target`.

The following remain separate: Office repairs; the Arcade cabinet/art pass; and the reported closed bathroom door’s side crack viewed obliquely from Arcade. Prior frontal acceptance does not disprove that crack. Newly authored interactive doors must start closed unless explicitly requested otherwise. Mirrors retain the established Full-with-reflections requirement. Hardware verification here covers M2 Pro/Metal; unsupported backends show their actual reason, but other hardware was not tested natively.

## Curated native images

Sixteen unaltered PNGs total 5,990,404 bytes. No frozen executable, profiling trace or redundant screenshot archive is retained.

- Fixture: [white before](../images/graphics-diagnostics/fixture-before-pink.png), [pink after](../images/graphics-diagnostics/fixture-pink.png), [blue after](../images/graphics-diagnostics/fixture-blue.png), [graded emission](../images/graphics-diagnostics/fixture-emission-on.png), [flicker dip](../images/graphics-diagnostics/fixture-flicker-dip.png), [metal with diffuser off](../images/graphics-diagnostics/fixture-metal-off.png).
- Matched textures: [High](../images/graphics-diagnostics/texture-high.png), [Medium](../images/graphics-diagnostics/texture-medium.png), [Low](../images/graphics-diagnostics/texture-low.png).
- Visibility: [mild before](../images/graphics-diagnostics/winter-before-mild.png), [mild after](../images/graphics-diagnostics/winter-mild.png), [moderate](../images/graphics-diagnostics/winter-moderate.png), [severe](../images/graphics-diagnostics/winter-severe.png), [clear sheltered room](../images/graphics-diagnostics/winter-sheltered-severe.png), [nearby warm lantern](../images/graphics-diagnostics/winter-near-lamp-severe.png).
- [Live GPU and RSS meter](../images/graphics-diagnostics/gpu-ram-meter.png).

Publication SHA/CI, reclaimed storage and custody release are recorded in the delivery message after verification and cleanup. Caffeinate PID 84362 keeps its existing idle-system and display assertions for the next owner.
