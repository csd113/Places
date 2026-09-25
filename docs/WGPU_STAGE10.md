# wgpu Stage 10: renderer parity validation and targeted repairs

Status: **Stage 10 is implemented and measured in the working tree.** The wgpu
renderer remains feature-complete relative to the OpenGL reference (Stage 9);
this stage added no rendering feature. It built the independent parity evidence
the migration needed, found and repaired four real ported-feature defects, and
recorded the bounded differences that remain.

**Stage 11:** the OpenGL/GLES2 renderer was removed from mainline and
preserved at the `renderer-gles2-reference` tag; the wgpu renderer described
here is the only implementation. See [RENDERER_REFERENCE.md](RENDERER_REFERENCE.md).

Read with [WGPU_STAGE9.md](WGPU_STAGE9.md) (the feature handoff),
[WGPU_LIGHTING.md](WGPU_LIGHTING.md), [WGPU_MATERIALS.md](WGPU_MATERIALS.md),
[WGPU_TEXTURES.md](WGPU_TEXTURES.md), [RENDERER_BOUNDARY.md](RENDERER_BOUNDARY.md)
and [VERIFICATION.md](VERIFICATION.md). The Stage 9 documents remain the record
of how each feature was ported; §2 below records what Stage 10 corrected.

## 1. What Stage 10 verified

* The canonical 50-view OpenGL↔wgpu gate (`tools/bench/capture_baseline_views.sh`
  + `compare_captures.py`) was re-run and improved on the Stage 9 numbers
  (§3). There are no views where the final metrics are worse than Stage 9's.
* A supplementary 40-view campaign
  (`tools/bench/capture_expanded_views.sh`) covers geometry junctions,
  long corridors, thresholds, the arch, stairs (top and underside), the
  balcony, the narrow home hall, near/distant geometry, every material class
  (matte, high-shine, normal-mapped, cutout, three glass kinds, reflective
  plastic/metal, emissive), bright/dark lightmap regions, prop contact
  darkening, probes and planar mirrors head-on and oblique, prop families,
  floor/wall/oblique/distant decals, long and height-dependent fog, and the
  UI in both profiles (§4).
* Intermediate products were compared directly, not inferred from final
  pixels: the lightmap atlas pages are **byte-identical** between backends for
  Full and Low (fresh bakes, `PLACES_DUMP_LIGHTMAPS=1`); the probe, planar,
  lightmap and bloom *contributions* (default minus feature switch) match the
  reference in magnitude, maximum and spatial correlation (§5).
* Live lifecycle: real SDL window resizes (larger, smaller, rapid, repeated),
  minimize and restore, `Full -> Low -> Full` rebuilds, repeated level
  rebuilds, a 900-frame run, capture and shutdown — all executed and clean
  (§6).
* The remaining Stage 9 residuals were investigated to a measured cause; one
  was a correctable capture-path conversion and one a correctable
  texture-space decision (§7, §8).

## 2. Stage 10 repairs (renderer parity, no feature work)

| Repair | Before | After | Evidence |
|---|---|---|---|
| Raw targets were cleared to the **linear** form of the reference's clear colour (Stage 8 left the sRGB value behind when Stage 9 made targets raw) | Canonical `pool_entry`/`wet_deck_shallow` showed the clear-colour region as `(2, 2, 2)` where the reference has `(20, 20, 23)`/graded `(18, 18, 21)`: a feature-shaped hole | Scene, planar and probe clears use the reference's raw `(0.08, 0.08, 0.09)`; only the sRGB surface paths use the linear form (`CLEAR_COLOR_SRGB`) | `low/pool_entry` over-8 share 0.13 % → 0.000 %, max 21 → 3; `low/wet_deck_shallow` max 21 → 4 |
| The presented/resolve target was sized with the **scene** target, so under Low the resolve and the HUD ran at ≤480 px and were upscaled | Low menu mean 1.580 / 1.19 % >8 (crisp reference HUD vs upscaled wgpu HUD) | Presented is the drawable; the resolve and HUD run at default-framebuffer resolution exactly like the reference; the scene target keeps the Low budget | Low menu mean **0.212** / 0.007 % >8 (Full 0.163); `[wgpu] post targets: scene 480x270 … presented 1280x720` |
| Base-colour textures were `Rgba8UnormSrgb`: hardware decode + shader re-encode + blending in linear space | A broad, texture-shaped +1 display level on minified surfaces (a convexity bias: the reference blends display bytes) | All textures (base colour and data) are raw `Rgba8Unorm`; the fragment assembly and filtering happen in the reference's display space, and the sRGB surface is the only conversion | 39/50 canonical views improved; mean-of-means 0.466 → 0.419 |
| `PLACES_CAPTURE` copied the presented image through an **sRGB** capture texture (hardware round trip) | Every measured pixel carried an avoidable hardware encode/decode | The post path copies the presented raw image into a raw `Rgba8Unorm` capture texture with no transfer function; the direct fallback keeps the surface-format path | Capture readback now byte-equal to the presented display values; the ±1 histograms unchanged (proving the readback was not the bias source) |
| A plan/fill lightmap failure was rebuilt a second time with `LightmapMode::Off` (re-baking `BakeConfig::HARD`) | `level0_pit` (a page-overflow fixture) diverged from the reference by mean 8.934 / 40.2 % >8: the reference keeps the *same* bake and mesh when the plan fails | Only an actual atlas-upload failure rebuilds; a plan/fill failure keeps the neutral build's historical vertex-lit mesh, exactly like `opengl/renderer.rs::build_level_for_load` | `level0_pit` mean **0.434** / 0.045 % >8, max 33 |
| A wgpu test asserted a false invariant (`every drawn surface resolves a base texture`, including fixture luminous faces and housings) | Failed in every asset configuration | The test now pins the real contract: architectural draws resolve catalog base textures; fixture faces use the fixture-sheet path and housings the fallback sheet | `the_shipped_demo_resolves_every_architectural_draw_to_a_base_texture` |

The OpenGL reference source, shaders, GL state and rendering behaviour were not
changed. No asset, level, material, prop or decal file was changed.

## 3. Canonical parity gate (50 views)

Captured with the historical asset root
(`tools/bench/baseline_asset_root.sh`) on macOS/Metal, 1280×720, default
settings, compared view by view against the OpenGL captures of the same asset
root (`compare_captures.py`; per-pixel worst RGB channel, 0..255).

| Metric | Stage 9 | Stage 10 |
|---|---:|---:|
| Smallest per-view mean | 0.140 | **0.103** (`low/drum`) |
| Largest per-view mean | 0.907 | **0.854** (`high/pool_entry`) |
| Mean of the 50 means | 0.466 | **0.419** |
| Largest share of pixels > 8 | 0.213 % (`high/office`) | **0.213 %** (same view, unchanged) |
| Largest single pixel/channel difference | 163 (`high/pool_wide`) | **163** (same view) |
| Views improved by >0.02 mean | — | 39/50 |
| Views regressed by >0.02 mean | — | **0/50** |

The OpenGL reference set re-captured from the historical asset root remains
**50/50 byte-identical** to `docs/renderer-baseline/` (`compare_baseline.py`),
and `docs/renderer-baseline/` was not regenerated.

## 4. Expanded campaign (40 views × Full/Low)

`tools/bench/capture_expanded_views.sh` captures supplementary views; the
Stage 10 run compared wgpu against OpenGL per view:

* 80/80 views captured, 0 failures.
* Per-view means 0.000 … 0.973; the highest are lightmap/bloom-heavy pool
  views (`pool_bright` 0.973, `decal_distance` 0.914) and the same class of
  residual the canonical set shows.
* The hottest 80×45 block in every view is 0–3/255: no feature-shaped region.
* The largest cluster above 40 is `low/glass_clear_pool` (714 px, max 68):
  a near-horizontal window-frame edge that the two rasterizers place one
  *scene* pixel apart (×2.67 in the Low upscale), the documented raster-tie
  class — the High version of the same view has max 17.
* A/B contribution checks: probe reflections (board 1.062 vs 1.057 mean, max
  5/5; metal 1.505 vs 1.498; linoleum 0.140 vs 0.140), planar (3.931 vs 3.922,
  max 32/32; head-on 2.026 vs 2.017), lightmap atlas (5.054 vs 5.040, max
  49/49; under-balcony 3.364 vs 3.352), bloom (0.067 vs 0.063 and 0.071 vs
  0.076). Contribution correlations: planar 0.9994, lightmaps 0.991–0.993,
  probes 0.993–0.994. An inverted or swapped reflection would decorrelate;
  these do not.

## 5. Intermediate-product validation

* **Lightmap atlas.** Fresh bakes of `places_demo` in both backends
  (`PLACES_DUMP_LIGHTMAPS=1`, separate state roots) produce byte-identical
  `places_demo_page0.png` and `places_demo_page1.png` for Full and Low. The
  shared neutral builder and cache are consumed unchanged; the wgpu renderer
  converts RGB8→RGBA8 only at upload (alpha 255, never read).
* **Probe faces.** The ignored GPU round-trip test
  (`render::wgpu::reflections::tests::the_cube_round_trip_matches_the_reference_face_convention`,
  run in the Stage 10 gate) pins face order, the `s` axis and the `t` axis
  with the real capture matrices. Content equality is bounded by the probe A/B
  above (magnitudes, maxima and correlation).
* **Planar capture.** The planar A/B above plus the v-flip/inside/on-plane CPU
  mirror test (`the_planar_sampling_is_the_reference_projection_with_the_capture_flip`).
* **Emission/bloom.** Canonical and expanded emissive views match; the bloom
  A/B above matches the reference contribution. The emissive and blur targets
  remain raw display space (`the_bloom_chain_is_raw_display_space`).
* **Post composition.** The resolve/present pipeline formats, settings
  identity, target sizes (scene = profile, presented = drawable) and
  single-conversion contract are pinned in `postprocess.rs` tests; the Low
  menu capture is the end-to-end check.

## 6. Lifecycle validation

Executed on macOS/Metal with the real SDL window (benchmark-gated
`PLACES_BENCH_WINDOW_CYCLE`, see §9):

* **Resize larger/smaller/rapid/repeated**: 640×360 → 800×450 → 500×300 →
  800×450 → 640×360 → 900×500 → 640×360 (logical). Every step recreated the
  surface, scene, depth, emissive/blur and presented targets; nine
  `[wgpu] post targets` rebuilds with matching drawable sizes and no
  validation errors. The post-lifecycle capture is identical to the same
  view's pre-lifecycle capture.
* **Minimize/restore**: real `minimize`/`restore` SDL calls under vsync
  pacing; the run completed, no validation/device/surface errors, and the
  post-restore capture is identical to the pre-lifecycle capture on both
  backends.
* **Level reload / repeated rebuild**: 8 scripted `Full -> Low -> Full`
  cycles in one 900-frame run, each through `Settings::set_quality` and the
  normal `set_level` rebuild path; `[settings] rebuilt GPU resources` appears
  exactly 8 times, zero errors.
* **Long run**: the isolated 900-frame run records 9 world uploads with
  byte-identical counts (6 411 vertices / 10 470 indices / 118 draws) and
  texture residency alternating exactly between Full 175 243 224 B and Low
  15 335 384 B — no growth across the 8 rebuild cycles and no error lines.
  Peak resident set 758 MB (wgpu) vs 582 MB (OpenGL); the difference is
  wgpu's own CPU-side caches and staging, and it is stable across the cycle.
  (An earlier run of the same matrix reported 567 MB because the window was
  occluded by parallel capture jobs for much of it, so many frames were
  skipped.)
* **Shutdown**: every run exits 0 through the ordinary path; captures are
  written before shutdown.

## 7. The +1 display-level residue

Stage 9 left a broad, approximately +1 residue and attributed it to the “sRGB
compatibility round trip”. Stage 10 falsified the alternatives and repaired
the parts that were defects:

1. **Per-texel sRGB decode + `linear_to_srgb` is exact.** A new ignored GPU
   measurement test
   (`render::wgpu::texture::tests::the_srgb_sample_round_trip_is_measured_on_this_adapter`)
   samples all 256 byte values through `Rgba8UnormSrgb` with `textureLoad`,
   re-encodes with the shader's exact IEC curve, and reads back: **0 error for
   all 256 values** on the Apple/Metal adapter. The round trip was never the
   source.
2. **The capture readback was one.** Stage 9 captured through an sRGB target.
   The readback is now raw (§2). Re-measuring showed almost no change to the
   histogram, which proves the residue is created by the render, not the
   capture.
3. **The capture texture and clear colour were real defects** and are fixed;
   the clear fix accounts for the entire visible `pool_entry`/`wet_deck`
   anomaly.
4. **The remaining residue is minification, not a global bias.** Flat,
   near-1:1 surfaces are byte-identical; the +1 appears only where textures
   are minified. The reference filters *display-space* bytes with driver-
   generated mips; wgpu now filters display-space bytes too (the raw texture
   repair), but its deterministic CPU box mip chain rounds half-up while the
   GL driver's implementation-defined filter rounds differently. The three
   candidate roundings were measured on an isolated Low/no-bloom reception
   capture against the reference: truncation mean 0.700 / signed bias −0.38,
   half-up (the Stage 6 rule, kept) 0.509 / +0.24, half-even 0.419 / +0.09.
   Half-even is the closer fit to this one driver, but the choice of a
   rounding rule for every backend from a single Metal/GL measurement would
   not be a backend-neutral correction, so Stage 10 kept the documented
   deterministic half-up chain and records the numbers instead. This is a
   driver-defined implementation, not a renderer contract; it is bounded and
   documented as an accepted backend raster difference.

## 8. Raster-edge differences

The isolated high-contrast edge differences remain the two-rasterizer class
Stage 9 recorded: individual pixels and 1–6 px clusters at silhouettes, the
largest bounded by the `low/glass_clear_pool` frame edge (one scene pixel, §4).
The canonical maximum single-pixel difference is unchanged at 163. Beyond the
edge ties, the canonical >8 population also includes soft emissive/bloom halos
at fluorescent panels and pool lights: the largest connected component is
`high/office`'s 952 px glow band (its 0.213 % over-8 share is unchanged from
Stage 9), followed by 617 px there and 146–220 px components in the pool
views. Those are brightness-envelope differences on emissive faces, not
displaced geometry or missing features: the hot 80×45 block metric stays at
0–3/255 across the set.

## 9. Diagnostics and benchmark semantics

* **`PLACES_NO_OFFSCREEN` stays OpenGL-only.** It selects the reference's
  direct-to-framebuffer diagnostic, which has no wgpu counterpart (wgpu always
  runs the offscreen post chain, the reference's own default). Stage 11 removes
  the path together with the OpenGL renderer; porting dead diagnostic
  architecture for symmetry would add a second unsupported frame path. Recorded
  in `render/facade.rs` and here.
* **`PLACES_BENCH_WINDOW_CYCLE=<frame>:<action>[,...]`** (new, benchmark-only)
  drives live window events: `resize:<w>x<h>`, `minimize`, `restore`. It goes
  through the real SDL window, so the platform's own events and drawable-size
  changes flow through the normal frame loop. `src/bench.rs` parses and tests
  it; `src/main.rs::apply_window_action` performs it.
* **`BENCH_SUMMARY` telemetry** reports the last frame the renderer actually
  submitted. In an isolated run (`PLACES_BENCH_NOSWAP=1`, Places Demo, 900
  frames with quality cycles) wgpu reports 150 draw calls, 156 total batches,
  29 299 visible vertices, 31 055 total vertices, 74 texture binds and 73
  material changes; OpenGL reports 150 draw calls, 159 total batches, 29 299
  visible vertices, 31 067 total vertices, 171 texture binds and 143 material
  changes. The count differences are accounting (OpenGL counts per-material
  binds, wgpu counts per-draw material records), not missing work; draw calls,
  visible vertices and visible batches agree exactly. A frame that is never
  submitted — `PLACES_BENCH_NORENDER`, or a surface acquisition that keeps
  returning `Skip` (e.g. while the window is occluded by another window) —
  reports no draw counts rather than a fabricated number; that is the
  contract, and it explains the all-zero Stage 9 observation (those runs ran
  while other capture jobs held the foreground).

## 10. Regression coverage added

* `postprocess.rs`: presented target is the drawable; scene stays
  profile-sized; stats include the presented size.
* `surface.rs`: raw and sRGB clear colours both pinned against the reference
  display value.
* `texture.rs`: every semantic samples raw display space;
  `the_srgb_sample_round_trip_is_measured_on_this_adapter` (ignored GPU
  measurement).
* `world.rs`: display-space sampling and single-conversion shader contract;
  the display-space product rounds to the reference byte for every authored
  texel; the fog formula CPU mirror (including the 12 m height cap); the
  planar projection/v-flip/inside/on-plane mirror; the architectural-draw
  base-texture test corrected.
* `lightmap.rs`: `needs_upload_fallback` pins that a plan/fill failure never
  re-bakes with `Hard` lighting.
* `bench.rs`: `PLACES_BENCH_WINDOW_CYCLE` parsing and frame selection.
* `tools/bench/capture_expanded_views.sh`: the durable expanded campaign.

## 11. Platform matrix

| Platform | Backend required | Status | Evidence |
|---|---|---|---|
| macOS | Metal | **VERIFIED** | Adapter `Apple M2 Pro`, backend `Metal`, surface format `Bgra8UnormSrgb`, present mode `Fifo`/`Immediate`; debug + release builds; full test gate; canonical and expanded parity; lifecycle; GPU measurements above |
| Linux | Vulkan | **NOT EXECUTED — ENVIRONMENT BLOCKED** | No Linux host available in this environment. Procedure documented in VERIFICATION.md; the wgpu setup already fails fast when the native adapter is absent, and `tests/test_wgpu_bootstrap.py` asserts the native backend on whatever host runs it |
| Windows | Direct3D 12 | **NOT EXECUTED — ENVIRONMENT BLOCKED** | No Windows host available in this environment; same procedure and gate as Linux |

Stage 10 is **not** fully cross-platform complete. The exact remaining
requirement is: run `sh tools/verify.sh` plus the capture/lifecycle commands on
a Linux/Vulkan host and a Windows/D3D12 host and record the selected backend,
capture metrics and lifecycle results in the matrix above. Nothing in this
stage claims those results.

### Cross-platform execution procedure

On each remaining platform, from a graphical session with the toolchain of
[VERIFICATION.md](VERIFICATION.md):

```sh
# 1. Build and confirm the native backend is the required one.
cargo build --release
python3 -m unittest tests.test_wgpu_bootstrap          # fails if not the native backend
PLACES_RENDERER=wgpu PLACES_LEVEL=places_demo PLACES_VERBOSE=1 target/release/places
#    -> "[renderer] wgpu | adapter: ... | backend: Vulkan|Dx12 | surface format: ..."

# 2. Full gate (fmt, clippy, tests, asset/texture/prop/package/editor/compiled-build).
sh tools/verify.sh

# 3. Canonical captures and per-view comparison (same-asset-root OpenGL control).
sh tools/bench/baseline_asset_root.sh
PLACES_ASSET_ROOT=$PWD/target/agent-work/baseline-assets/asset-root \
PLACES_CAPTURE_DIR=$PWD/target/agent-work/linux/vulkan-opengl PLACES_RENDERER=opengl \
    sh tools/bench/capture_baseline_views.sh
PLACES_ASSET_ROOT=$PWD/target/agent-work/baseline-assets/asset-root \
PLACES_CAPTURE_DIR=$PWD/target/agent-work/linux/vulkan-wgpu PLACES_RENDERER=wgpu \
    sh tools/bench/capture_baseline_views.sh
python3 tools/bench/compare_captures.py target/agent-work/linux/vulkan-wgpu \
    target/agent-work/linux/vulkan-opengl
# repeat with tools/bench/capture_expanded_views.sh

# 4. Lifecycle (resize/minimize/restore/quality cycles) as in §13.
```
Do not mark the platform verified from compilation alone, and do not accept
silently running wgpu's GL backend: the bootstrap suite's backend assertion is
the check.

## 12. Accepted bounded differences

* Minified texture sampling: driver mip filter and CPU box filter differ
  (§7.4). Bounded by the canonical metrics above.
* High-contrast raster edges: 1–6 px coverage ties (§8).
* Under Low, the scene target is the reference's ≤480 px budget; the resolve
  and HUD now run at the drawable's resolution, exactly like the reference.
* `PLACES_BENCH_NOINDEX` / `PLACES_BENCH_EXACT_VERTEX` remain OpenGL-only
  submission diagnostics (documented in `render/facade.rs`).
* The direct-to-surface fallback (no usable post targets) blends the UI on the
  sRGB surface; it is a first-frame/failure path only, unchanged by Stage 10.

## 13. Regression commands

```sh
# canonical gate (OpenGL must stay byte-identical; wgpu compared per view)
PLACES_ASSET_ROOT=$PWD/target/agent-work/stage10/baseline-assets/asset-root \
PLACES_CAPTURE_DIR=$PWD/target/agent-work/stage10/canonical-opengl \
PLACES_RENDERER=opengl sh tools/bench/capture_baseline_views.sh
python3 tools/bench/compare_baseline.py target/agent-work/stage10/canonical-opengl
PLACES_ASSET_ROOT=$PWD/target/agent-work/stage10/baseline-assets/asset-root \
PLACES_CAPTURE_DIR=$PWD/target/agent-work/stage10/canonical-wgpu \
PLACES_RENDERER=wgpu sh tools/bench/capture_baseline_views.sh
python3 tools/bench/compare_captures.py target/agent-work/stage10/canonical-wgpu \
    target/agent-work/stage10/canonical-opengl

# expanded campaign
PLACES_ASSET_ROOT=$PWD/target/agent-work/stage10/baseline-assets/asset-root \
PLACES_CAPTURE_DIR=$PWD/target/agent-work/stage10/expanded-wgpu \
PLACES_RENDERER=wgpu sh tools/bench/capture_expanded_views.sh
python3 tools/bench/compare_captures.py target/agent-work/stage10/expanded-wgpu \
    target/agent-work/stage10/expanded-opengl

# lifecycle (benchmark-only window actions)
PLACES_BENCH=1 PLACES_BENCH_FRAMES=34 \
PLACES_BENCH_WINDOW_CYCLE=3:resize:800x450,6:resize:500x300,8:resize:800x450,12:minimize,16:restore,18:resize:640x360,24:resize:900x500,26:resize:640x360 \
PLACES_BENCH_QUALITY_CYCLE=20:low,22:full \
PLACES_LEVEL=places_demo PLACES_RENDERER=wgpu target/release/places

# ignored GPU contracts
cargo test --all-features --bin places -- --ignored
```
