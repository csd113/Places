# Map storage and Static / Full lighting

12 October 2026. Native checks used Apple M2 Pro, 16 GiB RAM, Metal. MB is decimal;
MiB is binary. This pass changes prepared map data and lighting selection; original
GLBs, PNG artwork, authored presentation and external asset references stay intact.

Lighting now offers **Static** (historical vertex-lit `off`) and **Full**. Texture
quality/filtering preserve it; saved Medium becomes Full, while an explicit old
Low override retains Static. Static adds authored sky/global light to surfaces
and entities through bounded coarse visibility, suppresses material response and
reflections, and Full restores their saved preferences. New packages omit Medium
and Static reflection capture. Legacy readers remain; partial packages get an
`--variants off,full` rebuild command. Curated export rejects Full fallback.

PLLC1 stores 16×16 local ranges and RGB11 / signed moment XYZ10 in an eight-byte
word. Independent colour, energy and reconstructed-light bounds choose exact
six-half tiles when needed. Shape/framing and coefficients are validated with
bounded allocation. Nominal compact cost is 8.203125 bytes/texel; quality takes
priority. It expands to the established **RGBA16F pair, 16 bytes/texel/group on
GPU**, filtering linear coefficients before reconstruction. An exact KTX2 writer
remains available. [WebGPU’s format table](https://github.com/gpuweb/gpuweb/blob/main/spec/index.bs#L16286)
supports filtered Rgba16Float; this pass tests native Metal.

Loading retains shared half bytes and uploads them directly, removing full-float
atlas arrays and upload re-encoding. Decoded model images share `Arc` ownership;
changed/unreferenced models evict safely. Pending/displayed leases preserve active
records outside the 192 MiB LRU. PLPS1 shares exact prepared streams and verifies
segment/original hashes, retaining distinct placement/material/lighting/index
state. Segment fetches and working sets are bounded. Sharing must improve both
raw and Deflate-6 archive cost with conservative manifest overhead; otherwise
records stay literal. Geometry still expands per placement on CPU/GPU.

Full visibility edges add 8×8 to the retained 4×4 footprint (80 positions);
uniform areas remain 16. Density, gutters and shader filtering stay intact. The
independent diagonal oracle passes its ≥40% lower-error bound and thin openings
retain base samples. Current Zoo catalogue additions require twelve pages;
architectural holes are reused first, preserving formerly valid eleven-page
layouts. Zoo uses 192 MiB instead of the historical 176; Demo still permits ten
pages for its two groups.

**Complete packages: 645.109 → 367.235 MB, a 43.1% reduction (277,874,142 bytes).**

| Map | Before MB | Final MB | Final atlas MB | Bytes/texel | Full GPU MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Demo | 103.86 | 60.68 | 188.94 | 9.01 | 320 |
| Model Zoo | 136.15 | 83.09 | 117.55 | 9.34 | 192 |
| Pool | 17.85 | 8.97 | 19.31 | 9.21 | 32 |
| Home | 6.24 | 3.19 | 18.77 | 8.95 | 32 |
| Hollow | 158.00 | 84.48 | 127.08 | 11.02 | 176 |
| Winter | 80.04 | 42.59 | 78.38 | 10.68 | 112 |
| Beach | 32.36 | 19.83 | 75.40 | 8.99 | 128 |
| Aero | 26.40 | 15.35 | 39.33 | 9.38 | 64 |
| Arcade | 84.21 | 49.06 | 146.27 | 9.30 | 240 |

Raw lighting members across all packaged modes decreased **2,449.48 → 811.03 MB**.
Final weighted storage is **9.55 bytes/texel**; Hollow needs 11.02 and Winter 10.68.
The higher rates are automatic precision fallback, not forced eight-byte output.
All nine packages use exact prepared-stream sharing. CPU atlas pixels and GPU
atlas storage are now each the Full GPU column; the former CPU float pixels
were twice that size. Zoo needs its additional current-content page.

Before is the nine installed packages at handoff. Current Zoo includes prior
catalogue additions; Demo/Zoo use the already-refined Spooner-man. Those rows are
observed package changes, not a one-variable codec experiment. Home also has a
clean same-source baseline. No independent percentage estimates are added.
All final packages pass current source/dependency and complete record validation.

| Map / app cache | First usable scene s, before → after | Package-load phase s, before → after | Peak RSS MB, before → after |
| --- | ---: | ---: | ---: |
| Home / cold | 1.54 → 1.46 | 0.32 → 0.29 | 528 → 394 |
| Home / warm | 1.56 → 1.46 | 0.38 → 0.29 | 537 → 393 |
| Demo / cold | 4.67 → 4.02 | 3.52 → 2.85 | 2085 → 1703 |
| Demo / warm | 4.50 → 3.98 | 3.33 → 2.80 | 2222 → 1775 |
| Winter / cold | 2.61 → 2.54 | 1.47 → 1.36 | 1124 → 696 |
| Winter / warm | 2.63 → 2.51 | 1.48 → 1.37 | 1126 → 697 |

Values are medians of two cold/warm application-state repetitions in new
processes, High/Full, 640×360 logical / 1280×720 drawable. OS/GPU caches were
untouched; “cold” does not mean a cold filesystem. Trace time is native, not
browser latency. RSS covers unified CPU/GPU memory, not isolated CPU peak.
Atlas GPU costs remain sixteen bytes/texel. Total GPU counters and
Windows/Vulkan/browser execution were unavailable/unrun.

| Final build | Static prepare s | Full prepare/encode s | Probe capture s | Sharing s | Archive s | Peak process RSS MB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Model Zoo | 0.66 | 903.71 | 0.78 | 0.69 | 1.63 | 3536 |
| Demo | 21.19 | 600.15 | 1.28 | 1.21 | 1.48 | 3554 |
| Pool | 0.17 | 59.71 | 0.22 | 0.05 | 0.18 | 825 |
| Home | 0.04 | 36.15 | 0.00 | 0.05 | 0.08 | 439 |
| Hollow | 101.64 | 535.20 | 0.58 | 2.43 | 2.04 | 2831 |
| Winter | 13.37 | 276.66 | 0.48 | 1.00 | 1.01 | 2019 |
| Beach | 28.99 | 174.01 | 0.41 | 0.21 | 0.47 | 2577 |
| Aero | 0.21 | 186.34 | 0.33 | 0.21 | 0.34 | 1815 |
| Arcade | 0.23 | 354.55 | 1.63 | 0.29 | 1.09 | 2611 |

Matched Home default builds: **31.0 → 36.5 s** total; Full prepare/encode
**26.36 → 36.15 s**, with the former 4.09 s Medium stage removed. Peak RSS
**452.7 → 438.7 MB**. The small diagonal control Full build was **22.61 →
24.18 s** total (624.1 → 671.9 MB RSS). These phases include planning, transport
and encoding; they are not isolated ray-solver counters. The successful corrected
lineup took 55.3 minutes. One initial Zoo fallback was rejected; the current-source
capacity correction required replacing the first partial validation batch.

Bakes used eight workers sequentially. Finer coverage costs more despite Medium
removal; compact storage does not accelerate transport. Old-solve codec-only
encode/decode pilots cost 0.10/0.04 s Home, 1.22/0.44 s Demo, 1.07/0.42 s Arcade
and 0.32/0.13 s Winter. These exclude complete loading. Short repetitions and
single bakes have host jitter; no exhaustive campaign was run.

Matched codec-only native views (ten across Home, Demo, Winter and Arcade) differed
by at most one 8-bit display-channel value. Visual inspection found no snow/dark
banding or changed warm/cool/UV balance. The curated before/after pairs use the
same camera, authored exposure, pose, frame 30 and fixed 1/60-second step. Remaining
texel-scale shadow structure is visible at the retained physical density.

| Before | After |
| --- | --- |
| Static snow / retained cheaper appearance<br>![Before](../images/map-storage-lighting/static-snow-before.png) | ![After](../images/map-storage-lighting/static-snow-after.png) |
| Full diagonal caster / finer edge coverage, residual stairs visible<br>![Before](../images/map-storage-lighting/full-shadow-before.png) | ![After](../images/map-storage-lighting/full-shadow-after.png) |
| Full snow / retained cold light and gradients<br>![Before](../images/map-storage-lighting/full-snow-before.png) | ![After](../images/map-storage-lighting/full-snow-after.png) |
| Full UV room / retained colour and reactive pigments<br>![Before](../images/map-storage-lighting/full-uv-before.png) | ![After](../images/map-storage-lighting/full-uv-after.png) |

Capture inputs `(x,z,yaw) / pitch`: Winter `(-2.5,2,-69) / 19`; Arcade
`(7.3,22.5,322) / -28`. The shadow control explicitly sets `(x,y,z,yaw)` to
`(23.6,6.5,36.9,124) / -65`; its two-page data uses the same final coverage rule.
Winter interior retained its restrained appearance (maximum change 9/255);
closed-roof isolation also passes a bounded fixture. Exposed snow/props receive
authored sky. Home, Demo, ice, coloured arcade props, UV and
current Zoo were also inspected in both modes. The matched shadow view changes by at most 2/255, so its improvement is modest
at this distance; remaining 16-interval/m stair structure is not hidden by blur.

The live Arcade run verified **Static → Full → Static → Full**, then Low
textures → High textures without changing Full. Both Static captures and both
Full captures were pixel-identical; UV off/on and texture restoration returned
exact Full pixels. A spawned duck survived every transition and moved afterward.
The two texture refits reused the oversized active records (272.7 MB); no decoded
variant read was needed. Saved Medium lighting selected Full with Medium textures
and Full material response in a separate native run. Two small explicit headless
GPU tests passed repeated world/prop/character/material-resource transitions.
No total GPU leak counter was available; resource assertions and restoration passed.

Final local gates: formatting; strict workspace/all-target/all-feature Clippy
in debug and release; `cargo test --workspace` (2,262 library tests), all-features
tests (2,268), integration tests, source Python contracts (346 plus the new focused export guard), asset validation and patch whitespace. Production maps were
built only by explicit developer commands; hosted CI remains source-only. The
starting documentation CI 38184168719 passed. Publication SHA/CI are recorded in
the task delivery. Raw measurement/capture logs remain temporary rather than
being duplicated into tracked documentation.
