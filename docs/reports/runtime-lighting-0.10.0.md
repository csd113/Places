# Runtime lighting performance — 0.10.0 staging

The follow-up to [the doorway correction](performance-0.10.0.md) reduces the
matched close-door median from **11.546 to 10.541 ms** (86.6 to 94.9 FPS,
8.7% less frame time). The reproduced Full→Static update stall falls from
**1.765–1.811 seconds to 164–165 ms**. Four redundant 240 MiB lightmap
uploads disappear from each tested reflection/texture settings cycle.
All **14 matched native image pairs are pixel-identical**. There are still
transition stalls and an unexplained timing outlier; this is bounded evidence
on this Mac, not a general tail-latency or crash guarantee.

## Demonstrated causes and changes

- Entity direct lighting still copied a 112-byte source with dynamically
  indexed taps into fragment-local storage. The shader now reads the selected
  uniform fields and skips exactly zero source/tap visibility. Positive tap
  arithmetic, accumulation order, source selection, attenuation and falloff
  remain unchanged. These changes were measured together; register spills and
  their individual contributions were not isolated.
- Prop upload traced UV visibility for every vertex, including ordinary
  materials. A ten-second symbolized CPU sample during settings changes found
  1,925/5,442 main-thread samples in prop upload, 1,883 in blacklight visibility.
  Upload now ORs the referenced vertices of fluorescent primitives and queries
  only those vertices. Shared vertices retain coverage; invalid metadata uses
  conservative coverage. Source counts for Arcade's mixed reactive batches
  were 18,354 Static vertices but only 2,346 reactive vertices. The GPU-only
  pilot retained the approximately 1.8-second stall; selective UV work removed
  most of it. Skins and moving objects retain their existing update paths.
- Texture/reflection rebuilds reuploaded an unchanged immutable lighting atlas.
  Exact `Arc` identity plus resident-texture status now permits sharing its GPU
  handles. Full→Static clears CPU atlas identity; Static→Full and replacement
  resources upload normally. Cancellation drops handles without destroying the
  live resource. The tested cycle logs six atlas uploads before, two afterward,
  and four reuse events: **960 MiB less transfer and four fewer duplicate atlas
  allocations**, with unchanged 240 MiB steady atlas residency.
- The renderer shares immutable `LevelLightmaps` metadata instead of cloning
  it. Arcade's 47,960 chart records at 96 bytes each account for **4.39 MiB** of
  avoided chart allocation/copy per Full install. Packed texel data was already
  shared; this change does not save another complete CPU pixel payload.

Probe interpolation, direct-light ranking, caster/visibility cache revisions,
door invalidation, static transport, shadow rays, transparency and reflection
ordering were reviewed. No further demonstrated inefficiency justified changing
them. A proposed receiver cache was discarded: shipped moving/animated assets
have no fluorescence, so its state and invalidation complexity had no measured
workload benefit. Three small per-frame uniform writes were also left alone.
No map, asset, texture quality, light count, geometry or culling changes remain.
Current packages were reused without baking.

## Matched native measurements

13 October 2026: Mac14,9, M2 Pro, 19 GPU cores, 16 GiB, macOS 27.0.1
(26A434), native wgpu/Metal. High textures/filtering, Full lighting/reflections,
bloom, FOV 60°. Windowed trials use logical 1280×720, **2560×1440 drawable,
Retina 2×**, Immediate presentation. Close fullscreen uses logical 1512×949,
**3024×1898 drawable**, Fifo/VSync. A new benchmark dimension guard rejects
changed drawable sizes; an earlier mismatched fullscreen series was excluded.
No owned build ran during timing and no thermal warning was reported.

Simulation advances 1/60 second, with 120 ready frames of warm-up. Two reversed
before/after pairs provide 1,200 frames per windowed condition and 720 fullscreen
frames. GPU samples are deduplicated per run; asynchronous results are not
paired with the current CPU row. Values are milliseconds, median / p95 / worst.
CPU loop includes drawable waits and pacing; update is a separate CPU interval.

| View | Loop before | Loop after | GPU elapsed median before → after | CPU update median before → after |
|---|---:|---:|---:|---:|
| Closed bathroom door | 4.638 / 5.627 / 14.747 | 4.162 / 5.165 / 14.759 | 5.088 → 4.606 | 1.121 → 1.052 |
| Bathroom lateral path | 4.505 / 6.118 / 15.883 | 4.059 / 5.570 / 17.164 | 4.879 → 4.405 | 1.068 → 1.026 |
| Close fullscreen door | 11.546 / 12.024 / 13.832 | 10.541 / 11.156 / 12.987 | 12.093 → 11.072 | 0.792 → 0.824 |
| Lit Arcade cabinets | 1.908 / 2.853 / 15.390 | 2.498 / 8.854 / 23.196 | 2.255 → 2.204 | 0.887 → 0.907 |
| Dark Office room | 1.988 / 2.992 / 22.438 | 1.970 / 2.698 / 19.396 | 1.521 → 1.439 | 1.223 → 1.240 |
| Winter outdoors | 1.618 / 3.077 / 20.138 | 1.426 / 2.939 / 14.208 | 1.910 → 1.823 | 0.234 → 0.245 |
| Bathroom mirrors | 2.019 / 3.545 / 15.293 | 1.942 / 3.224 / 17.433 | 2.164 → 1.961 | 0.859 → 0.852 |

One cabinet after-run had 8.183 ms loop / 5.418 ms GPU medians, while its first
repeat was 1.941 / 2.140 ms. Two further reversed controls returned loop medians
1.963 and 1.904 ms after versus 2.052 and 1.868 ms before; GPU medians were
2.159 and 2.146 versus 2.273 and 2.261 ms. The original outlier is retained above
and unexplained. Winter's individual runs also overlap; there is no claim of a
uniform benefit in light workloads. Draws/triangles match: door 274/73,104,
lateral median 275/73,224, close 247/69,794, cabinets 148/27,414, Office 26/7,760,
Winter 296/76,383, mirrors 209/37,600. The mirror view retains its planar pass.

Three raw Metal timestamp samples confirm non-overlapping accounting: seven
overlapping spans summed to 31.306 ms but covered 6.570 ms; the other unions
were 4.961 and 4.683 ms. **GPU elapsed includes scheduling stalls**, not active
utilization. A separate two-second Metal System Trace, excluded from timing,
covered 296 scene encoders in a stable 1.3-second window: active GPU interval
union averaged 4.134 ms per encoder. Metal allocation time-median/peak was
677,871,616 bytes (646.5 MiB); this final-only diagnostic cannot establish a
before/after allocation-peak reduction. Exported tables replace the bulky trace.

## Settings, visual acceptance and remaining limits

Two matched cycles exercised Full→Static→Full, Low→High filtering, reflections
off→Full, then Low→High textures while retaining Full lighting. Full→Static's
CPU update changed 1,765.497/1,811.231→164.155/165.121 ms; subsequent loop gaps
changed 1,770.589/1,816.247→166.330/167.340 ms. The first cycle's complete
request→world-commit durations changed:

| Transition | Before → after, ms |
|---|---:|
| Full→Static | 4,017 → 732 |
| Static→Full | 3,310 → 1,713 |
| Reflections off / Full | 1,984 / 1,980 → 226 / 222 |
| Low / High textures, Full lighting | 2,172 / 2,315 → 417 / 581 |

These durations include background preparation and uploads, not one frozen
frame. Filtering applied directly. Full→Static still stalls approximately
165 ms; High texture return still stalls **272–277 ms** in update. Worker
preparation/decode and remaining resource creation are not eliminated.

Native comparisons cover static surfaces, colored lights, cabinets and UV
carpet, doors/windows, mirror reflections, dark Office props, Winter snow and
illuminated windows. Separate scripted captures verify door open/close,
fixture off/on, crate spawn/move/despawn, a dark mannequin actor's spawn/move/
despawn, Static/Full lighting, and Low textures with Full lighting. All 14 RGBA
2560×1440 pairs have zero differing pixels and zero maximum channel difference;
representative originals were visually inspected. Scripted/capture runs are
excluded from steady measurements. No missing illumination, stale resources,
leaks or visible popping appeared in these bounded samples. Same-process
Arcade→Office→Winter→Arcade reloads completed with matching package identities.
The prior intermittent gameplay crash was not reproduced or solved.

Curated native images: [cabinets](../images/runtime-lighting-0.10.0/cabinets.png),
[Static fluorescent gifts](../images/runtime-lighting-0.10.0/gifts-static.png),
[dark actor](../images/runtime-lighting-0.10.0/dark-actor.png),
[Winter](../images/runtime-lighting-0.10.0/winter.png),
[mirrors](../images/runtime-lighting-0.10.0/mirrors.png).

## Reproduction and verification

Baseline commit `5843390f8da7cd20db1e5918de9bbe89d587f645`, retained binary
`local/performance-0.10.0/bin/places`, SHA-256
`8b827df137d67e7315ff8b13bbf18d0644b40de3d799d0c66eeee6d50a23a159`.
Final measured binary: `local/lighting-runtime-0.10.0/bin/places`, SHA-256
`f1dc2d911e8c7aa38876e4b8b9130cda185f04627a4264ce02fc713eb1318ddb`.
Arcade package remains `fa9abf3fd0e45842dff2212b6a41787ef56a8529becdb495572561a3f3d373fe`.
Local evidence retains `delivery-*` receipts/CSV/logs, all paired originals,
campaign/reload scripts, CPU sample, compact summaries and Metal exports.
Cameras: door `−1.6,1.6,2.8 / 90,0`, close `−.65,1.6,2.8 / 90,0`,
cabinets `7.4,1.6,17.8 / 306,5`, Office `21,1.6,10 / 0,−3`,
Winter `1,1.6,10 / −32,−3`, mirrors `−1.65,1.6,5.45 / 180,−14`.
Lateral retains the prior 3 m/s trajectory, Z approximately 1.65–4.05.

```sh
python3 tools/bench/frame_time.py --binary local/lighting-runtime-0.10.0/bin/places \
  --out local/new-lighting-door --expected-drawable 2560x1440 --frames 600
# Add --motion for lateral; substitute the retained baseline for before.
python3 tools/bench/frame_time.py --binary local/lighting-runtime-0.10.0/bin/places \
  --out local/new-lighting-close --spawn=-0.65,1.6,2.8,90 --fullscreen --vsync \
  --expected-drawable 3024x1898 --frames 360
```

Exit-zero validation: configured formatting and locked all-target/all-feature
check; strict Clippy in dev and release; `cargo test --workspace` (2,265 library,
two compiler CLI, seven integration) and `--all-features` (2,271 library plus
the same CLI/integration). Two inexpensive CPU fixtures protect selective UV
work/shared indices/fallback and immutable atlas identity/residency. The existing
ignored native GPU helper passed explicitly in 0.44 s with one additional
zero-source witness in the same dispatch/readback (464 bytes). The other 18 GPU
tests were not explicitly run. Python metrics (14), art-style hero (six),
`py_compile`, benchmark help and malformed-dimension rejection passed.

Publication SHA/hosted CI, executable identity and cleanup custody are recorded
in the delivery message and local `publication.json`. Prior evidence is retained.
