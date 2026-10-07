# Prompt 6 — Light Snowfall Weather

Winter now normally loads gentle snowfall through `"weather": {"kind": "snow"}`.
The renderer has no Winter map-id special case; omitted weather preserves every
other shipped Place's defaults. Prompt 6 alone is implemented on `Winter-expansion`.
Prompts 7–9 remain queued and separate. Starting HEAD was
`3abf087715b6401cfbd8504c7debd0ff8df11a03`; its
[consolidation CI](https://github.com/csd113/Places/actions/runs/37594023563)
completed successfully before this work.

## Configuration and rendering

`SnowfallDef` exposes validated seed count, volume radius/height, X/Z wind,
flake size/speed ranges, opacity and a catalog material. Defaults are 1,400 seeds,
16 m radius, 12 m height, `[0.18, 0.06]` m/s wind, 2.5–7.5 cm flakes, 0.45–1.05 m/s
downward fall and 0.85 opacity. High/Medium/Low evaluate stable prefixes of
1,400/1,050/700 seeds. Count is capped at 2,048. See the canonical weather table
in `docs/MAP_AUTHORING_GUIDE.md` for all accepted ranges.

The existing stateless steam effects, wgpu billboard pipeline, material cache,
depth testing and buffer uploads are reused. Snow adds one material draw, not
a new shader, pipeline, bake or map-wide particle simulation. Independently
seeded sizes/speeds/phases and curved drift avoid identical straight tracks.
Analytic positions remain world-anchored under camera movement; camera-local
tiles wrap only inside fully transparent boundary bands. Near, distance and
vertical fades provide depth and suppress popping. Frustum/fade rejection runs
before shelter checks and GPU upload. Actual authored buffers are allocated
once, including disabled steam; the combined maximum is 10,240 quads within
the existing `u16` index format. Quality rebuilds preserve the snow clock.
Winter's snow-only GPU buffers reserve 201,600 vertex bytes and 16,800 index
bytes; 221 submitted flakes upload only 31,824 vertex bytes that frame.

All non-open room ceilings use their actual flat/gable heights to suppress
flakes below their footprints. Occluding `void_walls` cover roof slabs and
porches. An outer 0.35 m fade softens drift across shelter edges; outside snow
remains visible through an open doorway from indoors. This is deliberately a
small architectural shelter model: arbitrary prop roofs, tree canopies and
dynamic roofs are not ray-traced. Author a room ceiling or slab for those.
There is no accumulation or snow collision; ground rejection uses opaque depth.

`assets/core/textures/effects/snowflake_01.png` is a committed 256×256 RGBA sheet,
with full 0–1 UVs, zero-alpha borders, white body and a blue-grey contrast rim.
It uses a normal blend material without emission. The built-in `imagegen` skill
generated the master from a prompt for one irregular white snow granule with
a subtle cool rim, transparent background, no scene/text/shadow/glow and ample
padding. The master is retained; resizing and clearing faint edge-alpha noise
were mechanical contract corrections. No texture imagery is generated at runtime.
The contract is documented in `docs/ASSET_SPECIFICATION.md` §7.1a.

## Native appearance, movement and measurements

The campaign used the real SDL3/wgpu player on Apple M2 Pro / Metal with fresh
isolated state, accepted settings, 960×540 logical / 1920×1080 drawable windows,
and confirmed committed level/renderer quality. High/Medium/Low captures cover
the square, pale ground, dark sky, lodge interior, entering/leaving a lodge,
held strafing and camera turns. Additional captures use the maintained
`debug-maps/snowfall-20261007/sources/snowfall_contrast.json` against a bright
white floor. Inspected frames show sparse varied flakes against both light and
dark backgrounds, clean indoor walls and outside snow visible through the door.

Doorway movement traversed Z −3.5 to −12.8 m and back, including the 0.6 m deck
step. The strafe traversed approximately 9.8 m while turning. Roof suppression
removed up to 68 otherwise visible High flakes. The revised forest capture
turns before walking into a tree; the earlier opaque-trunk endpoint remains in
raw evidence. Native quality cycling follows High → Low → Medium → High,
records all four budgets in one CSV, and keeps a monotonic snow clock with
no reset. CPU regressions also assert world-position continuity and invisible
wraps, downward varied motion, shelter edges and bounded scratch capacity.

| Quality | Seeds evaluated | Peak submitted in Winter campaign | Largest projected quad coverage | Worst view p95 CPU sync |
| --- | ---: | ---: | ---: | ---: |
| High | 1,400 | 221 | 0.794% | 340 µs |
| Medium | 1,050 | 161 | 0.743% | 323 µs |
| Low | 700 | 109 | 0.430% | 149 µs |

Every recorded capture/quality-cycle run has zero vertex/group capacity growth.
Coverage sums projected quad rectangles, including transparent texels and
fragments rejected by opaque depth; it is a conservative fill estimate, not
a hardware fragment counter. Diagnostic sync includes shared steam/billboard
generation, projected-corner calculations and queue submission. Projection
diagnostics are disabled in normal play and the A/B runs. The allocation claim
covers retained seed/scratch/group storage; wgpu staging and unrelated engine
allocations are outside it.

Same-binary on/off A/B uses the Winter square, VSync off, GPU completion,
120 warm-up frames and 600 measured frames per run, in on/off/off/on order.
Diagnostics are disabled on this production path. Each side contributes 1,200
frames per quality:

| Quality | Snow off median frame | Snow on median frame | Median difference | Mean difference |
| --- | ---: | ---: | ---: | ---: |
| High | 1.734 ms | 1.792 ms | +0.058 ms | +0.137 ms |
| Medium | 1.511 ms | 1.577 ms | +0.066 ms | +0.132 ms |
| Low | 1.384 ms | 1.419 ms | +0.035 ms | −0.027 ms |

Host jitter explains the negative Low mean difference; it does not indicate a
speedup. These are bounded native observations on one Mac, not a cross-device
GPU guarantee. The recorded A/B binary `3814016f…` is preserved alongside its
manifest. The final `24411ead…` binary changes only benchmark CSV flushing;
the normal rendering path is identical and final quality/contrast/turn checks
use it. Initial captures with rejected incomplete settings are retained as
superseded evidence and excluded from these measurements. A failed debug-map
discovery attempt is also retained; the corrected harness mounts its package
in an isolated runtime payload and asserts the actual installed map.

## Verification and preservation

The initial required `cargo test --workspace` passed: 2,014 library cases,
23 existing ignored, six binary/integration cases and doctests; library duration
298.78 s. Strict workspace/all-target/all-feature Clippy and focused Rust snow
tests passed. Python weather/Winter tests passed eight cases. Asset validation
passed 292 entries / 166 placeable assets / five themes with zero warnings;
texture validation passed 69 textures with 60 soft provenance warnings, including
the new authored ImageGen sheet. No quality setting, test or lint was weakened.

The final command
`env -u PLACES_ASSET_ROOT RUSTC_WRAPPER= CARGO_INCREMENTAL=0 sh tools/verify.sh`
exited 0. It passed formatter, locked workspace/all-target/all-feature check,
development and release strict Clippy, locked workspace/all-feature Rust tests
(2,014 library + six binary/integration, 23 existing ignored; 298.71 s library),
release build, asset validators/generator checks, 196 Python cases across the
suites, focused showcase/static-lighting tests, the explicit ignored atlas-plan
test, all four bundled currentness/decode gates, both explicit native Low-lighting
GPU tests, and `git diff --check`.

The compiled-build suite passed all 10 cases in 41.060 s with zero skips. The
wgpu bootstrap suite reported 26 cases in 66.620 s, with 11 presentation cases
skipped by its existing macOS lock guard (`CGSSessionScreenIsLocked=Yes`). No
guard or assertion was changed. After the user unlocked the Mac, the isolated
`env -u PLACES_ASSET_ROOT python3 -m unittest tests.test_wgpu_bootstrap` rerun
passed all 26 cases in 150.115 s with zero failures and zero skips on native Metal.
It used the existing matching release binaries and packages: no Cargo build,
bundled rebake or full aggregate rerun was performed. Binary and all five bundled
package hashes remained unchanged. All 196 aggregate Python cases have therefore
executed successfully across the original gate and this focused completion.
Earlier Winter/contrast/movement checks also ran on the actual native Metal
renderer with zero capture skips. No presentation cases remain pending; the
rerun log, fixture traces and final status are retained in the existing evidence.
Intentional failing-tool fixtures print error text while their assertions pass.

Winter was normally compiled once in 107.315 s (53,540,951 bytes), then passed
decode and `verify --require-current`. Comparison with the preserved baseline
proves all geometry vertex/index bytes, collision, navigation, lightmap texels,
irradiance, lighting, props and probes remain unchanged. Of 17 existing payload
blobs, 12 are byte-identical; the three mesh records only shift 14 architecture
material indices by +1 because weather adds a material-table entry, and two
lightmap metadata records only update their content key. The new weather
semantics/dependency are the intended addition. The normal aggregate compiler
gate refreshes the four other bundled packages once because `compiler.rs`
intentionally includes the exact full catalog hash in both full-build and
reusable lighting-stage keys. No cache key is rewritten to bypass that gate,
no force build is used, and no compiler optimization is included in Prompt 6.
The subsequent normal Winter build reports `current` in 1.057 s and performs
no second bake.
All 86 non-manifest payloads in the four refreshed non-Winter bundles are
byte-identical to their preserved baselines: Demo 32, Model Zoo 18, Movement
Test 18 and Lantern Hollow 18. Only their developer identity manifests change.
Their one normal build durations were 165.097 / 189.570 / 36.103 / 238.115 s
respectively. Every integrated Office, painting, aurora, winter, lighting and
movement asset/source remains intact.

`debug-maps/snowfall-20261007/evidence/` retains pre-snow and final matching
assets, original PNG master, sources, playable Winter/contrast packages,
pinned player/compiler, SDL library, launch instructions, raw native telemetry,
captures and verification logs outside `target/`. Existing debug maps and both
external Office/consolidation evidence archives remain intact. Shared build
artifacts remain available for queued prompts. Publication SHA, exact remote
verification, CI and final process/ownership release are recorded in the final
handoff and accompanying ignored `evidence/handoff.json`.
