# wgpu Stage 9: complete renderer feature migration

Status: **Stage 9 is implemented in the working tree.** The wgpu backend now
contains every rendering feature of the OpenGL reference: the lightmap atlas
(the normal/default baked-light path), static reflection probes and the planar
mirror, props/GLB models, dynamic objects, fixture geometry and emission, the
emissive bloom chain and resolve, decals, fog and the renderer-owned HUD — all
at the reference's own feature-level behaviour under both quality profiles.

**Stage 11:** the OpenGL/GLES2 renderer was removed from mainline and
preserved at the `renderer-gles2-reference` tag; the wgpu renderer described
here is the only implementation. See [RENDERER_REFERENCE.md](RENDERER_REFERENCE.md).

This document is the Stage 9 handoff. It complements
[WGPU_LIGHTING.md](WGPU_LIGHTING.md) (Stage 8: the baked lighting model this
stage extends to the atlas, the sheen and the display-space assembly),
[WGPU_MATERIALS.md](WGPU_MATERIALS.md), [WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md),
[WGPU_TEXTURES.md](WGPU_TEXTURES.md) and [RENDERER_BOUNDARY.md](RENDERER_BOUNDARY.md),
and it records the measured comparison against the OpenGL reference in
[`docs/renderer-baseline/`](renderer-baseline/BASELINE.md).

> **Stage 10 note (2026-09-24).** The Stage 10 parity validation found and
> repaired four real deviations from this stage, and re-measured everything:
> the raw scene/planar/probe clears (this document's §5), the presented
> target's resolution under Low (§7), base-colour textures returning to raw
> display space (§5), the `PLACES_CAPTURE` readback, and the lightmap
> plan-failure fallback. The canonical metrics improved; see
> [WGPU_STAGE10.md](WGPU_STAGE10.md) §2–§4. The Stage 9 text below remains the
> record of what this stage built and why.

## 1. What Stage 9 added

| Feature | Where | Reference behaviour reproduced |
|---|---|---|
| Lightmap atlas | `render/wgpu/lightmap.rs`, `world.rs`, `world.wgsl` | `LightmapMode::On` build, content-keyed bake cache (shared with OpenGL on disk), two RGB8 pages uploaded as raw `Rgba8Unorm`, page selection by the vertex byte, `u_lightmap_enabled`/`u_light_scale` |
| Reflection probes | `render/wgpu/reflections.rs`, `renderer.rs` | up to two 64/32-texel cubemaps baked at `centroid + 1.2 m`, six GL-ordered faces, nearest-probe selection per frame, fallback black cube |
| Planar mirror | `render/wgpu/reflections.rs`, `renderer.rs` | half-size target, Full only, mirrored view-projection, reversed front face, mirror-range exclusion, `on_plane`/`inside` sampling |
| Props / GLBs | `render/wgpu/props.rs` | one buffer pair per `(model, cell)` batch, one draw per primitive, clamped model sheets, per-primitive emission, opaque-only |
| Dynamic objects | `render/wgpu/dynamic.rs` | model-space meshes, `u_model` via the environment uniform, per-object `u_light_scale` probe, opaque |
| Fixtures + emission | `world.rs` draw set, material uniforms | `SurfaceKind::Light` faces/housings, per-vertex emission, emission masks, animated `u_emission_scale` |
| Bloom / resolve | `render/wgpu/postprocess.rs`, `post.wgsl` | scene target (Full = drawable, Low ≤ 480 wide; the *presented* target is the drawable since Stage 10), shared-depth emissive target, quarter-size two-pass 5-tap blur, resolve (bloom add, exposure, tone shoulder, grade), plain present copy |
| Decals | `render/wgpu/decals.rs`, `decals.wgsl` | generated atlas + external sheets, depth bias from `glPolygonOffset(-1,-4)`, alpha cut-out, last pass in the body |
| Fog | `world.wgsl` | the reference's squared-exponential distance/height fog, after emission, before the display conversion |
| HUD | `render/wgpu/ui.rs`, `ui.wgsl` | the 480×272 reference HUD over the presented image, depth test off, straight-alpha blend |

## 2. Lightmaps

The wgpu level build asks the neutral builder for
`LightmapBuildOptions::for_profile(quality, LightmapMode::On)` when lightmaps
are requested (the default), exactly like `build_level_for_load`. The neutral
bake, atlas planner, fill and content key are unchanged and shared with the
OpenGL path; the wgpu renderer owns its own `LightmapCache::with_disk()`, so it
restores the same `cache/lightmaps/v4-<hash>` entries OpenGL wrote (measured:
cold 68 s, warm 19.8 s in a debug build; the cache key is byte-identical).

* Atlas pages are raw `Rgba8Unorm` (wgpu has no sampleable RGB8); the alpha byte
  is 255 and never read.
* `WorldVertex` carries `lightmap_uv` as `Unorm16x2` and `lightmap_page` as a
  plain float (the un-normalised byte), matching the reference's exact
  attribute types in both of its layouts. Stride 64 bytes, pinned by tests.
* `surface_light()` samples `mix(lm0, lm1, step(0.5, page))` only when
  `lightmap_enabled * (1 - step(254.5, page)) > 0.5`, then multiplies by
  `light_scale`. `LIGHTMAP_NONE` keeps the historical vertex-lit colour exactly.
* Sheen, reflection and emission are all scaled by that same light factor, so a
  dark room darkens them exactly as the reference does.
* Profiles: Full 16 texels/m on 1024² pages with the quincunx bake, Low 9
  texels/m on 512² pages with the single-tap bake (both from the neutral
  `QualityProfile`).

## 3. Reflections

### Probes

Baked once per level load, after the decal upload, with the reference's full
scene body (static, props, dynamics, decals). Face matrices are the reference's
GL cube order/up table. Two conventions had to be pinned:

* **Face row order.** A GL FBO stores a face bottom-up; WebGPU targets store
  their first row at the top. The probe projection therefore negates NDC `y`
  (storing the reference's bottom-up image), and the capture pipeline uses the
  reversed front face to compensate for the winding flip — the same device the
  reference uses with `glFrontFace(GL_CW)` during a planar capture.
* **Bake position.** The routing's centroid is lifted `+1.2 m` (`PROBE_LIFT_M`),
  the reference's own lift.

Both are verified by
`render::wgpu::reflections::tests::the_cube_round_trip_matches_the_reference_face_convention`,
a GPU round-trip test (ignored by default) that captures a world-space quad with
the real capture matrices and samples it back, checking layer selection, the
`s` axis and the `t` axis.

### Planar

One plane per frame (the nearest whose reflective bounds survive the cull, Full
only), half the render size, its own depth, cleared to the reference's clear
colour. The capture skips the mirror's own static batches
(`material_plane == capture_plane`), without which the deck fills its own
reflection image. Sampling uses the reference's projected `uv`, with one WebGPU
correction: `v` is flipped because the capture target's first row is NDC `+y`.

## 4. Props, dynamics, fixtures

The neutral build's `PropMeshBatch` list is uploaded verbatim: world-space,
per-vertex-lit vertices, one draw per primitive, model sheets through the
texture cache under `Catalog` lifetime with clamp wrap and the player's filter.
Materials are plain-opaque with the primitive's emission (and its mask); props
never use normal maps, alpha modes or reflections, exactly like the reference's
`SurfaceState::plain`.

Dynamics mirror the reference's path: one small model-space buffer per model, one
group-3 environment per object carrying its model matrix and its baked-light
probe (`u_light_scale`), refreshed by `update_dynamic` only when an object moves.
The washer-drum demonstration is spawned by `set_dynamic_demo`.

Fixture luminous faces are `SurfaceKind::Light` ranges with per-vertex emission;
their housings draw the shared white sheet. A fixture's light remains entirely in
the CPU bake — the emission term is visual only and never illuminates anything.

## 5. Colour space: raw display-space targets

The single most important Stage 9 decision: every offscreen colour target
(scene, presented, planar, probe faces) is **raw `Rgba8Unorm`**, and the world,
decal and UI shaders write the reference's display-space values directly. Only
the surface is sRGB, converted once at the final copy. This is what makes
hardware alpha blending (glass, decals, the HUD), texture filtering, bloom and
reflection sampling all operate on the same values the OpenGL reference used.
The sRGB entry points (`fs_main`, `fs_cutout`, `fs_present`, `fs_resolve`) exist
for the direct-to-surface fallback and the final presented copy.

Two direct consequences, both measured against the reference:

* the translucent pass's blending matches (the glass-heavy views were the
  largest residual before this change, mean 2.7 → 0.8);
* the UI's semi-transparent panels blend correctly (the menu frame matches at
  mean 0.258).

## 6. The frame

```text
render_scene:
  ensure depth / pipelines / post targets / planar target
  select planar plane (Full only) + nearest probe
  planar capture (mirror ranges excluded, modes zeroed, capture environment)
  update material reflection modes + environment uniform + all cameras
  encode:
    scene pass        → raw scene target + depth   (static, props, dynamics, decals)
    emissive pass     → raw emissive target        (when an emissive draw survived)
    blur ×2           → quarter-size raw targets
    resolve/present   → raw presented target
  submit
render_ui:
  UI blends into the raw presented target (drawable in both profiles since Stage 10)
  presented copied to the sRGB surface with one encode
present: queue.present (or the acquired frame dropped under NOSWAP)
capture: re-render the chain into presented, replay the last UI list, copy to
         the capture texture and read it back
```

## 7. Quality matrix

| Feature | Full | Low | Source |
|---|---|---|---|
| Atlas: texels/m, page edge, padding | 16, 1024, 2 | 9, 512, 1 | `LightmapConfig::for_profile` |
| Bake taps per axis / prop-occlusion cell | 2 (quincunx) / 0.075 m | 1 / 0.15 m | `QualityProfile::bake_config` |
| Surface response (normal map + sheen + reflection strength) | drawn | gated off | `draws_surface_response` |
| Scene resolution | drawable | ≤ 480 wide, aspect preserved | `scene_target_size` |
| Presented/resolve resolution | drawable | drawable (scene size in Stage 9; corrected in Stage 10) | `target_sizes` |
| Planar reflection | enabled | disabled | `Reflections::set_profile` |
| Probe face edge | 64 | 32 | `probe_face_size` |
| Surface/fixture/decal sheets, masks, prop sheets | 1024/1024/1024/512/256 | 256/256/256/128/128 | `QualityProfile::budget` |
| Fog, emission and its animation, decals, UI | identical | identical | shared code paths |

## 8. Verification (working tree)

Measured on macOS/Apple Silicon (Metal), debug build, baseline asset root
(`tools/bench/baseline_asset_root.sh`), against `docs/renderer-baseline/`:

| Check | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| clippy (strict command, see §9) | clean |
| `cargo test --workspace --all-features` | 977 passed, 4 ignored, 24 failed — every failure classified in §9.1 |
| wgpu vs OpenGL, 25 views × Full + Low (default settings) | largest mean difference **0.907**, smallest 0.140; only a handful of views have any pixel above 8/255 (max 0.21 % on one view) |
| Menu frame (UI) wgpu vs OpenGL | mean 0.258, 0.01 % of pixels above 8 |
| Live `Full -> Low -> Full` (both backends) | seven `[settings] rebuilt GPU resources` rebuilds, exit 0, no validation error, no panic |
| OpenGL reference, unchanged source | 25/25 Full + 25/25 Low byte-identical against the baseline asset tree (release build) |
| GPU probe round-trip test (`--ignored`) | passes |

The scripted live switch used for the runtime matrix is the benchmark-only
`PLACES_BENCH_QUALITY_CYCLE=<frame>:<profile>[,...]` hook, which routes through
`Settings::set_quality` and therefore the renderer's normal rebuild path.

Representative per-view means (Full): corridor 0.368, drum 0.438, home main
north 0.611, wet deck 0.684, office 0.751, pool entry 0.925. The residue is
dominated by textures sampled at extreme minification (thin reveals/panels) and
a few raster-tie edges; no view shows a feature-level mismatch.

### 8.1 Test-failure classification

All 24 failures are pre-existing working-tree asset-state failures (the white
sheet is 1024×1024 where tests assert the historical 2×2 contract; several prop
GLBs in the working tree fail to parse and the renderer degrades to placeholder
boxes, failing the real-geometry assertions). They fail identically without any
Stage 9 code path involved, and the two wgpu-module ones
(`wgpu::texture::tests::the_fallback_is_the_committed_two_by_two_white_sheet`,
`wgpu::world::tests::the_shipped_demo_resolves_every_drawn_surface_to_a_base_texture`)
assert the same historical asset state. The set shrank from the Stage 8
baseline's 28 because the working tree's assets changed during the session
(see §11).

## 9. Tests added

* Lightmaps: page conversion/order, truncated-page padding, sampler policies,
  the page/fallback CPU mirror of `surface_light`, the raw atlas format, the
  64-byte vertex layout with `Unorm16x2`/`Float32` lightmap attributes, the
  192-byte environment uniform layout.
* Reflections: the six face directions/ups, 90° projection with the Y flip, the
  planar mirror composition, half-size target, `+1.2 m` bake position, the
  nearest-probe/plane rules, the capture colour format, and the ignored GPU
  cube round-trip.
* Props/dynamics: vertex conversion, emission records, alpha classification,
  transform finiteness, empty uploads.
* Decals: depth-bias mapping, fragment cut-off, sheet selection/orientation,
  pipeline state.
* Post: blur kernel/step, target sizes, settings identity, resolve maths,
  presented/present pipeline formats.
* UI: ortho corners, viewport maths, blend factors, font format, raw entry
  point.
* Integration: the five world pipeline variants and their states, the emissive
  flag propagation, material reflection-mode rule, wrap/sampler selection.

## 10. Resource and performance sanity

* The atlas is a level resource: 2 pages (1024² or 512², raw RGBA8) created at
  level upload, dropped with the level; the sampler and layout are renderer-wide.
* Probes: ≤2 cubemaps (6 faces of 64²/32² raw RGBA8) baked at load (12 scene
  submissions); the planar target is half the render size, recreated only on a
  size change.
* Props/dynamics: one vertex/index pair per batch/model, created at level load;
  per frame only the material uniforms of moving objects are written.
* Post: scene + presented + emissive + two blur targets, recreated only on a
  size/profile change; the resolve and blur bind groups are created with the
  targets. No per-frame pipeline, texture, buffer or bind-group creation.
* The debug-build canonical run captures each view in a few seconds with a warm
  lightmap cache; draw counts for Places Demo: 118 static + 37 prop + 1 dynamic
  + 3 decal + UI.

## 11. Pre-existing working-tree conditions left untouched

The tree carried in-progress asset edits at the start of Stage 9 (deleted glass
sheets, many modified PNGs/GLBs, the 1024² white sheet). During the session the
tree changed independently of this work: the three glass PNGs were restored and
12 textures returned to their committed bytes (file mtimes 16:01–16:02), which
is why four asset-state tests that failed at the Stage 8 baseline now pass. This
was not done by the Stage 9 work and was not reverted; Stage 9 changed no asset
file.

## 12. Known limitations

* The GPU round-trip orientation test is `#[ignore]`d because it needs an
  adapter; it is run explicitly as part of the stage verification.
* Texture minification at extreme LOD still differs on a few sub-pixel features
  (the same class Stage 8 recorded): the wgpu cache's CPU mip chain and the
  reference's driver-generated chain differ slightly.
* The direct-to-surface fallback (no post targets) blends the UI on the sRGB
  surface, where the reference blends display values; the fallback is used only
  for a first frame or a failed target, and the post path is the parity path.
* `PLACES_NO_OFFSCREEN` is not implemented for wgpu: it always runs the
  offscreen post chain (the reference's own default). The switch remains an
  OpenGL-only diagnostic.
