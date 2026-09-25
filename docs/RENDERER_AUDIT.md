# Places Renderer Audit — Stage 2

Status: **audit complete** (Stage 2 of the renderer-modernization plan).
Audit date: 2026-09-24. Audited revision: the Stage-1-cleaned working tree
(`main`, commit `4ad6d97` plus uncommitted Stage 1 cleanup), desktop
OpenGL/GLES2 renderer, crate `places` 0.7.0.

> **Stage 3 note (2026-09-24).** The working tree now implements the renderer
> boundary this audit called for: `src/render/common/**` is renderer-neutral,
> `src/render/opengl/**` owns every GL object, and the engine talks to
> `render::Renderer` only. Module paths below (`src/render/renderer.rs`,
> `render/reflections.rs`, `render/view.rs`, …) describe the Stage 2 tree; the
> Stage 3 mapping and ownership rules are in
> [RENDERER_BOUNDARY.md](RENDERER_BOUNDARY.md). Stage 3 changed no renderer
> behaviour, shader, asset or level format, so every behavioural finding in
> this audit still applies — only locations and visibility changed.

**No renderer behavior was changed to produce this document.** This audit is
read-only with respect to the renderer: no shader, GL state, material,
lighting, shadow, lightmap, reflection, decal, transparency, quality-mode,
asset-format or level-format change was made. `docs/RENDERER_AUDIT.md` is the
only intentional repository change.

Terminology used throughout:

- **GL** means the OpenGL/GLES2 API as accessed through the `glow` crate.
- “**current renderer**” means the renderer in the working tree described
  above, not any earlier commit.
- `Needs runtime confirmation` marks behavior that static inspection cannot
  prove; the uncertainty is explained at the point of use and collected in
  §27.

Source references are given as `path — Type::method`; line numbers, where
present, come from the audited working tree and are advisory only.

---

## 1. Purpose and scope

This document is the authoritative Stage 2 specification of the existing
Places OpenGL/GLES2 renderer. It exists so that later stages can:

1. **separate the renderer from the engine** (Stage 3 — establish the renderer
   boundary), and
2. **reproduce the renderer faithfully in wgpu** (Stage 4+), with parity
   determined against `docs/renderer-baseline/` and the verification gate in
   `docs/VERIFICATION.md`.

It deliberately documents rather than designs: it records what the renderer
*does*, every significant GPU resource it owns, every shader it compiles, every
pass it executes, the GL state each pass relies on, the texture-unit semantics,
the coordinate conventions, the low/high quality differences, and the places
where OpenGL specifics leak into engine code.

The document also records bugs, implicit state dependencies and architectural
smells discovered during the audit. Per the Stage 2 scope rule, **none of them
were fixed**; they are catalogued in §26 and consolidated as migration risks in
§25.

What this document is not:

- It is not a wgpu design. It does not assign bind groups, bindings or WGSL
  resources.
- It is not a proposal to change the renderer. Stage 3 owns the boundary;
  Stage 4 owns wgpu.
- It does not treat the historical Stage 0 baseline document as current
  behavior; where the baseline is quoted it is marked as historical.

### How to read this document

| Question | Section |
|---|---|
| What does the renderer do at a glance? | §2 |
| Which file does what? | §3 |
| What happens at startup / level load / frame / resize / quality change / shutdown? | §4 |
| What are the render passes, in order? | §5 |
| What GL calls exist and who owns them? | §6 |
| What GPU resources exist and how long do they live? | §7 |
| What does each shader expect and compute? | §8 |
| What vertex formats and geometry exist? | §9 |
| What GL state does each pass set or inherit? | §10 |
| How do textures, samplers and texture units work? | §11, §12 |
| How do materials, lighting, shadows, lightmaps, reflections, decals, transparency, normals and UI render? | §13–§21 |
| What changes between Low and Full? | §22 |
| What are the coordinate conventions? | §23 |
| Which code is renderer-independent / preparation / OpenGL? | §24 |
| What is easy to get subtly wrong in a port? | §25 |
| What is known to be odd or unverified? | §26–§27 |
| What must the new renderer reproduce? | Feature parity checklist |

---

## 2. Executive renderer overview

### 2.1 In one paragraph

Places is a first-person exploration game. Its renderer draws a static,
baked-lit interior: level geometry is emitted on the CPU from level JSON plus a
material catalog, baked once by the lighting system (lightmap atlas or
vertex-lit fallback), packed into quantized vertex buffers, and drawn in a
small number of passes into an offscreen scene target. An emissive-only pass
into a full-scene-sized target (which shares the scene depth buffer) feeds a
two-pass separable blur at quarter resolution for bloom; a
resolve pass applies exposure, a tone shoulder, a subtle grade and the bloom;
the result is presented to the default framebuffer, and the 480x272-reference
HUD is drawn afterwards with the same world shader. Optional per-material
planar reflection and static probe-cubemap reflection are drawn before the main
body (planar is Full-profile only; probes are baked at level load). There are
no realtime lights and no shadow maps: all lighting, including fixture pools
and contact/ambient occlusion, is baked into the lightmap atlas or vertex
colors.

### 2.2 Top-level data flow

```text
levels/*.json  +  assets/catalog.json  +  assets/**/*.png
        │                     │
        │             materials::resolve_materials        (CPU: decode, intern)
        ▼                     ▼
   LevelDef  ────────►  MaterialTable (RawImage per material texture)
        │
        │   lighting::LevelLighting::bake_with            (CPU: baked lights,
        │                                                  occlusion, shadows)
        ▼
 render::build_level_geometry*  ──► LevelMesh (Vertex)  ──► lightmap atlas pages
        │                                 │
        │                                 ├─ LightmapCache (memory + cache/lightmaps)
        │                                 ▼
        │                          MeshPacker chunks (PackedVertex)
        │                                 │
        │   props::resolve_instances      │   decals / fixtures / dynamic
        ▼                                 ▼
 Renderer::set_level  ──► GPU uploads: VBO/IBO, material/lightmap/prop/fixture/
        │                              decal textures, reflection probe bake
        ▼
 FrameLoop::render_and_present
        │
        ▼
 Renderer::render_scene  ──► SceneTarget (offscreen RGBA8 + depth)
        │                       │
        │                       ├─ reflect: probe cubemaps (baked) / planar target
        │                       ├─ emissive pass → scene-sized target (+scene depth)
        │                       │     → blur ×2 (÷4 ping-pong) → bloom texture
        │                       └─ resolve (bloom + exposure + tone + grade)
        ▼
 present to default framebuffer  ──► Renderer::render_ui (default FB, 480x272 UI)
        │
        ▼
 SDL_GL_SwapWindow (unless PLACES_BENCH_NOSWAP)
```

### 2.3 Entry points

| Role | Location |
|---|---|
| Application bootstrap and frame loop | `src/main.rs — bootstrap`, `FrameLoop::frame`, `FrameLoop::render_and_present` |
| Renderer construction | `src/render/renderer.rs — Renderer::new`, `Renderer::from_startup` |
| One-time GL resources | `src/render/renderer.rs — StartupResources::create` |
| Level upload + bake | `src/render/renderer.rs — Renderer::set_level`, `rebuild_level_geometry`, `build_level_for_load` |
| Frame render | `src/render/renderer.rs — Renderer::render_scene` |
| UI render | `src/render/renderer.rs — Renderer::render_ui` |
| Screenshot readback | `src/render/renderer.rs — Renderer::capture_default_framebuffer` |
| Settings → renderer | `src/main.rs — FrameLoop::apply_pending_settings`, `rebuild_graphics_resources` |

### 2.4 Major renderer modules

| Module | Role |
|---|---|
| `src/render/renderer.rs` | The GL renderer: programs, uniforms, buffers, textures, passes, captures, state |
| `src/render/view.rs` | Shader sources, drawable/viewport math, texture-unit and probe/bloom constants |
| `src/render/framebuffer.rs` | Offscreen scene target and the presentation quad/matrix |
| `src/render/postprocess.rs` | Emissive bloom capture, blur, resolve, post settings |
| `src/render/reflections.rs` | Probe cubemaps, planar mirror target, mirror math, routing |
| `src/render/mesh.rs` | Vertex layouts, quantization, surface keys, batch ranges |
| `src/render/geometry.rs` | Static world mesh emitter |
| `src/render/architecture.rs` | Generic architectural geometry (stairs, ramps, columns, rails…) |
| `src/render/fixtures.rs` | Fixture geometry and luminous face sheets |
| `src/render/props.rs` | Prop instancing/lighting/packing |
| `src/render/dynamic.rs` | Moving objects with their own upload/update path |
| `src/render/decals.rs` | Generated decal atlas, external sheets, decal quad emission |
| `src/render/animation.rs` | Emission pulse/flicker animation |
| `src/render/atmosphere.rs` | Fog parameters |
| `src/render/api.rs` | Level-geometry entry points usable by game/tests |
| `src/render.rs` | Module root: build orchestration, material lookup, upload helpers, projection |

### 2.5 Main GPU resources

Programs: **world**, **world-cutout**, **decal**, **present**, **resolve**,
**bloom blur** (6 programs from 2 vertex + 5 fragment sources; one
compile-time variant).

Targets: **scene target** (RGBA8 color + depth renderbuffer), **emissive
target** (bloom source, shares the scene depth), **blur A/B** (quarter size),
**planar reflection target** (half scene, own depth), **probe cubemaps** (64 or
32 texels/face, own depth).

Textures: material albedo/emission-mask/normal maps (catalog or pack), fixture
sheets, prop model sheets, lightmap atlas pages (RGB8, ≤2), decal sheets,
generated font atlas, white fallback, black cube fallback.

Buffers: static level VBO/IBO chunks, prop VBO/IBO chunks, dynamic mesh
VBO/IBO, UI VBO, present/bloom quad VBOs.

### 2.6 Main passes

Offscreen path, in order: scene target clear → planar reflection capture (Full
only, ≤1 plane) → scene body (opaque static → props → dynamic → cutout →
translucent → decals) → emissive capture into the bloom source → two blur
passes → resolve/present to the default framebuffer → depth clear → HUD.

### 2.7 Major features that must be reproduced

Baked lighting and lightmaps with a vertex-lit fallback; multi-page lightmap
atlas with a per-vertex page selector; static probes and half-resolution
planar reflections; normal mapping and view-dependent sheen (Full only);
emission masks, fixture luminous faces and emission animation; bloom from
emission; fog; distance-independent decals with polygon offset; alpha cutout
and translucent surfaces; dynamic objects; the 480x272 HUD; Low/Full profiles;
screenshot readback; the direct (no-offscreen) fallback path. Full checklist in
§“Feature parity checklist”.

---

## 3. Renderer source map

Classification column uses the Stage 3 vocabulary: **independent** (CPU/game
data not tied to OpenGL), **preparation** (CPU-side render data), **GL**
(directly owns or manipulates OpenGL). See §24 for the full analysis.

| File | Responsibility | Classification |
|---|---|---|
| `src/main.rs` | SDL init, window/GL context creation, frame loop, settings application, one-shot capture, benchmark integration | mixed: independent game loop + GL context/drawable/capture calls |
| `src/display.rs` | Window fitting to usable display bounds | independent |
| `src/settings.rs` | Persisted runtime settings (quality, bloom, reflections, lightmaps, filtering, FOV, window) + `PLACES_*` startup overrides | independent (values consumed by renderer) |
| `src/bench.rs` | `PLACES_BENCH_*` switches, frame timing CSV, camera override, skip render/swap/finish, no-cull/no-index/exact-vertex | independent + renderer switches |
| `src/quality.rs` | `QualityProfile` (Full/Low), texture-class budgets, lightmap/bake config, profile predicates | independent (renderer policy input) |
| `src/loader.rs` | Level discovery/loading (JSON, pack zip, embedded), material/fixture-sheet resolution, `LevelManager`, session `TextureCache` | independent + preparation |
| `src/level.rs` | Level schema/validation/geometry definitions (`LevelDef`, walls, floors, ceilings, props, lights, decals) | independent |
| `src/assets.rs` | Asset catalog, shipped-texture dimension contracts, package/state roots | independent |
| `src/materials.rs` + `materials/{resolve,image,pack,reflection,emission,response,decal}.rs` | Material definitions, resolution to texture indices, PNG decode, emission/reflection/alpha/response data | independent + preparation (decoded `RawImage`s) |
| `src/lighting.rs` + `lighting/{light,color,math,tuning,bake,occlusion,visibility,lightmap/*}.rs` | Baked lighting simulation, occlusion/shadow taps, lightmap atlas bake/fill/plan/cache | independent + preparation |
| `src/props.rs`, `src/gltf.rs` | Prop catalog/instances, GLB parse, embedded image decode | independent + preparation |
| `src/spatial.rs` | AABBs, cell grid, spatial queries | independent |
| `src/font.rs` | HUD font atlas generation (procedural pixels) | preparation (PNG policy exception, §26) |
| `src/ui.rs` | HUD/settings/main-menu vertex generation + cache | independent (vertices) |
| `src/perf.rs` | Performance overlay vertex generation and metrics | independent |
| `src/game.rs`, `src/input.rs`, `src/collision.rs` | Player/game state, input, collision | independent |
| `src/render.rs` | Module root: build entry points, material lookup, geometry/texture upload helpers, projection, decal emission, chunk upload | preparation + GL (uploads) |
| `src/render/api.rs` | `build_level_geometry*` entry points used by the app and audits | preparation (calls GL only via renderer uploads) |
| `src/render/mesh.rs` | Vertex layouts, quantization, surface keys, mesh packing, batch/spatial split | preparation |
| `src/render/geometry.rs` | Static level mesh emitter (floors, ceilings, skirts, walls, fixtures, decals) | preparation |
| `src/render/architecture.rs` | Generic architectural emitters (ramps, stairs, half walls, columns, archways, guardrails, thresholds, baseboards) | preparation |
| `src/render/fixtures.rs` | Fixture profiles and luminous-face geometry/sheets | preparation |
| `src/render/props.rs` | Per-model prop batch building, CPU per-instance transform and vertex lighting | preparation |
| `src/render/dynamic.rs` | Dynamic object scene, CPU transforms, model-space vertices | preparation + GL upload (via renderer) |
| `src/render/decals.rs` | Decal atlas generation, material slots, UV rects, quad emission | preparation |
| `src/render/animation.rs` | Emission pulse/flicker evaluation | preparation |
| `src/render/atmosphere.rs` | Fog constants/state | preparation |
| `src/render/view.rs` | Shader source constants, drawable size/UI viewport/FOV math, unit constants, probe/bloom divisors | GL (shader sources, unit numbers) + independent math |
| `src/render/framebuffer.rs` | `SceneTarget` creation/recreation, present quad + matrix | GL |
| `src/render/postprocess.rs` | `PostProcess` programs/targets, bloom blur, resolve | GL |
| `src/render/reflections.rs` | Probe targets, planar target, mirror math, routing metadata | GL (targets) + preparation (routing) |
| `src/render/renderer.rs` | All remaining GL state, programs/uniforms, buffer and texture upload, draw passes, reflection/probe capture, capture readback | GL |
| `src/render/tests.rs` | Shader/attribute/unit/layout pinning tests and renderer regression tests | test-only GL usage |
| `src/*_audit*.rs` | Developer diagnostics (opt-in, ignored tests); no shipping behavior | diagnostics |
| `src/perf.rs` | Performance overlay text/bar vertex generation, toggled with `-` | independent (UI vertices) |

Evidence: module doc comments at the head of each file; `src/render.rs` module
declarations and re-exports; `docs/VERIFICATION.md` for how the tree is gated.

Note: the level editor (`level-editor/`) has its own WebGL1 preview renderer
that is **not** part of the game renderer and is not a wgpu port target
(Appendix A of the shader audit, §8.12).

---

## 4. Renderer lifecycle

This section follows one process from launch to shutdown. Every claim is
traceable to the functions named; see §5 for the pass graph itself.

### 4.1 Startup (before any level)

`bootstrap` (`src/main.rs`):

1. Package root resolution and `set_current_dir`; SDL hints (`SDL_VIDEO_X11_WMCLASS`,
   `SDL_APP_NAME`).
2. `Bench::new()` parses `PLACES_BENCH*` switches.
3. `Settings::load_or_default()` then `apply_startup_overrides`
   (`PLACES_QUALITY`, `PLACES_NO_BLOOM`, `PLACES_NO_REFLECTIONS`,
   `PLACES_NO_LIGHTMAPS`, `PLACES_VSYNC`); `ensure_saved()`.
4. Window creation requests **OpenGL ES 2.0** (`GLProfile::GLES`, 2.0,
   double-buffered, 24-bit depth) and falls back to a desktop compatibility
   context (2.1) if the window cannot be created; the window is centred,
   resizable, `allow_highdpi()`, optionally desktop-fullscreen. No multisampling
   and no stencil are requested. `display::fit_window_to_bounds` keeps the
   window inside the usable work area.
5. The SDL event pump is not started until the frame loop; no GL calls occur
   before `Renderer::new`.

`Renderer::new` (`src/render/renderer.rs`):

1. Re-asserts GL attributes; `window.gl_create_context()` (with the same
   Compatibility fallback), `window.gl_make_current`.
2. Builds `glow::Context` via SDL's `gl_get_proc_address`.
3. Loads `PropCatalog::load_default` and the white sheet
   (`core:tex_white_01`, falling back to the embedded PNG `WHITE_SHEET_PNG`).
4. `StartupResources::create` performs all one-time GL work in this order:
   `glEnable(DEPTH_TEST)`, `glDepthFunc(LEQUAL)`,
   `glClearColor(0.08, 0.08, 0.09, 1)`;
   compiles **world** and **cutout** programs (`create_scene_programs`) and
   verifies the World program's attribute slots (`scene_attribute_locations`);
   compiles the **present** program and creates its quad VBO;
   creates the white texture, the font atlas texture (procedural),
   the generated decal atlas texture, and the decal program; creates the UI
   VBO; tries `create_post_process` (resolve + blur programs, quad VBO) and
   degrades to `None` on failure (plain copy presentation); creates the 1x1
   black cubemap.
   Sampler unit assignments happen here (`ProgramUniforms::bind_samplers`).
5. `from_startup` records defaults: offscreen enabled unless
   `PLACES_NO_OFFSCREEN=1`; packed vertex layout; indexing and culling on;
   `QualityProfile::Full`; linear filtering; bloom/reflections/lightmaps
   requested on; lightmaps not resident; both lightmap slots = white;
   empty caches; `FogState::SHIPPED`; and calls `refresh_post_settings`.

No framebuffer, renderbuffer, scene target, probe, planar target or bloom
target exists at startup, and no draw has occurred. There is also **no
capability probing** (no `GL_MAX_*` queries): the renderer assumes 8 vertex
attribute slots, texture units 0–6, a cubemap at least 64 texels, and
renderbuffer sizes at least the drawable.

`create_renderer` (`src/main.rs`) then applies settings in this order:
`set_quality` → `set_lightmaps_requested` → `set_bloom_enabled` →
`set_reflections_enabled` → `set_level` (first and only build of the boot
level) → `set_dynamic_demo` (demo only) → `set_texture_filtering` →
`set_culling(!bench.no_cull())` → `set_indexing(!bench.no_index())` →
`set_vertex_layout(Packed|Exact)`.

Two ordering consequences (documented, not fixed):

- The boot level is built before `set_indexing`/`set_vertex_layout`, so
  `PLACES_BENCH_NOINDEX` / `PLACES_BENCH_EXACT_VERTEX` do not affect the boot
  level build; they affect only later `set_level` calls.
- If the saved filtering is `nearest`, the boot level's textures are uploaded
  linear and then re-filtered in place by `set_texture_filtering` without
  re-uploading pixels (converges, §10.4).

`configure_vsync` runs after the renderer exists (context current); it sets
the SDL swap interval and reports the backend value.

### 4.2 Level loading

The CPU side (`src/loader.rs — LevelManager`) reads the level JSON, validates
it, resolves materials (`resolve_materials`) and fixture sheets, and keeps a
session `TextureCache` of decoded PNGs, so re-loading the same level does not
re-decode.

`Renderer::set_level` then, in order:

1. `rebuild_level_geometry`:
   - `build_level_for_load`: choose `LightmapMode::On|Off` from
     `lightmaps_requested`; run `LevelLighting::bake_with(level,
     profile.bake_config())` for On, `BakeConfig::HARD` for Off; resolve prop
     instances (decodes GLB images through `PropAssets`); emit the static mesh
     (`build_level_geometry_mesh[_with_lightmaps]`); look up the lightmap cache
     or bake and fill the atlas, writing the cache; then **upload the lightmap
     pages before geometry** (`upload_level_lightmaps`, or
     `clear_lightmap_pages` when off). An upload failure deletes partial
     textures, restores white pages, and rebuilds the whole mesh with
     `LightmapMode::Off` + `LightmapFailure::Upload`.
   - `spatial_grid = spatial_cell_grid(level)` (optionally overridden by
     `PLACES_CELL_METRES`).
   - keep a clone of `LevelLighting` as `dynamic_lighting` (CPU probe sampling
     for dynamic objects).
   - `pack_static_batches` (CPU) and `pack_prop_batches` (uploads per-model
     prop textures; packs per `(model, spatial cell)` index lists).
   - `upload_chunks` for static then prop buffers: reuses existing
     VBO/IBO pairs, converts `Vertex` → `PackedVertex` when the packed layout is
     active, deletes surplus pairs from a larger previous level.
   - fills `LevelBuildStats`.
   - `resolve_scene_extras`: derive reflection routing from the mesh
     (`reflections::routing_from_mesh`), `reflections.set_routing` (destroys
     the previous probe cubemaps), apply the quality profile, arm
     `pending_probe_bake`, resolve emission animations, reset
     `animation_seconds`.
2. `load_decal_sheets`: upsert external decal PNG sheets (GPU cache by key;
   failure → generated atlas or diagnostic texture).
3. Free the previous level's pack textures (`level_textures`), preserving the
   white and font textures.
4. Upload one GPU texture per `MaterialTable::textures` entry: catalog/missing
   textures into the session `surface_textures` cache, pack textures into
   per-level `level_textures`, through `fit_texture` (quality budget) and
   `create_texture_2d(repeat=true, linear)`; upload failure → white texture.
   `material_textures` is the parallel handle vector;
   `materials = MaterialRenderState::from_table(...)`.
5. `upload_fixture_sheets`: one sheet per `FixtureKind` slot, white when
   absent; catalog sheets cached session-wide, pack sheets per level.
6. If `pending_probe_bake`: `bake_reflection_probes` — up to 2 probe cubemaps,
   six faces each, each face a full `draw_scene_body` submission with
   reflection sampling suppressed. **This is real rendering during level
   load** (§5.3).

GPU work during load: lightmap uploads, decal/material/fixture/prop texture
uploads, lightmap and geometry buffer uploads, probe bake draws. Nothing is
drawn to the window during load.

### 4.3 Frame

`FrameLoop::frame` (`src/main.rs`): timing update → perf overlay update →
event pump → settings application → display status refresh → player movement →
`renderer.update_dynamic(dt)` (CPU transforms only) → `render_and_present`.

`render_and_present`:

1. Read `window.drawable_size()`; if empty, reset timing, sleep ~16 ms and
   return (no render, no present, no bench record).
2. `renderer.set_drawable_size(drawable)` — records only.
3. Choose camera: player position/yaw/pitch for Playing/Paused/PauseSettings;
   spawn position/yaw, pitch 0, for menus; `PLACES_CAMERA=yaw,pitch` overrides.
4. Unless `PLACES_BENCH_NORENDER=1`, `renderer.render_scene(cam…)` — §5.
5. If the filtering setting changed, `renderer.set_texture_filtering(...)`
   (in-place texture re-filtering, no re-upload).
6. Build/reuse UI vertices from `UiGeometryCache`; append the perf overlay if
   visible; `renderer.render_ui(vertices)` (skipped when NORENDER).
7. `PLACES_BENCH_FINISH=1` → `renderer.finish()` (`glFinish`).
8. One-shot capture: if `frame_count >= PLACES_CAPTURE_FRAME` and
   `PLACES_CAPTURE` is set, `capture_default_framebuffer()` → PNG → stop.
   Capture runs after UI and before swap, and is not gated on `skip_render`.
9. `window.gl_swap_window()` unless `PLACES_BENCH_NOSWAP=1`.
10. Bench record; stop at the configured frame count.

`Renderer::render_scene` is described pass-by-pass in §5.2–§5.8.

### 4.4 Resize

There is no SDL resize-event handler. Instead:

1. `refresh_display_status` compares the live `window.size()` with the saved
   settings; a manual windowed resize is adopted into settings (suppressed for
   one frame after a programmatic window change via `window_apply_in_flight`).
   HiDPI/backing-scale changes are logged once.
2. Every frame, `render_and_present` reads `window.drawable_size()` and calls
   `renderer.set_drawable_size`. This is a pure setter returning whether the
   value changed; it recreates nothing.
3. `render_scene` recomputes the projection and frustum from the *render size*
   every frame (no cached projection to invalidate).
4. `ensure_scene_target` deletes and recreates the scene target when the
   desired size changes (`framebuffer::scene_target_size(quality, drawable)`;
   Full = drawable, Low = ≤480 wide, aspect preserved, never upscaled).
5. `ensure_planar` recreates the planar target when the scene size changes;
   `PostProcess::ensure_targets` recreates bloom targets when the scene size
   changes.
6. The UI viewport is recomputed per frame from the drawable
   (`DrawableSize::ui_viewport`), so HUD scaling follows immediately.
7. Minimized/hidden windows report a zero drawable; those frames are skipped.
   `SceneTarget::create` additionally refuses zero sizes.

The scene target's depth attachment prefers `DEPTH_COMPONENT24` and retries
`DEPTH_COMPONENT16`; the chosen depth is logged once per creation. The planar
and probe targets always use 16-bit depth.

`Needs runtime confirmation`: the macOS live-drag sequence where SDL reports a
zero drawable mid-resize; static analysis says those frames are skipped and the
next non-zero drawable recovers, but that exact sequence is untested.

### 4.5 Quality-mode changes

A live Low/Full change runs `FrameLoop::rebuild_graphics_resources`:

1. `renderer.set_quality(profile)`: updates post settings, planar-reflection
   enablement (Full only), re-applies the reflections switch, invalidates the
   cached surface state.
2. `set_lightmaps_requested`, `set_bloom_enabled`, `set_reflections_enabled`.
3. `renderer.release_profile_textures()`: deletes and clears prop, surface,
   fixture-sheet and decal-sheet GPU texture caches.
4. `renderer.set_level(current_level)`: full re-bake/re-upload at the new
   profile (lightmap density/page size/taps, texture budgets, probe face size).
   The game world (player, camera, pause state) is untouched; this is not a
   level load.

Quality-dependent behavior is listed exhaustively in §22. Notably, the scene
target resizes lazily on the next `render_scene`, bloom targets on the next
`capture_bloom`, and the planar target is retained but disabled in Low.

Documented flag: `rebuild_graphics_resources` does **not** re-sync the dynamic
scene, so `DynamicMeshGpu` submeshes can hold GL texture names deleted at
step 3 while `set_level` uploads new ones; the dynamic scene revision is
unchanged, so dynamic draws use the stale handles. `Needs runtime confirmation`
(visual/GL-error manifestation); §26 Q3.

### 4.6 Level reload

Both the menu/`PLACES_LEVEL` path and the quality rebuild call
`Renderer::set_level`; reuse and destruction are identical except that the
level selector path calls `spawn_level_demonstration` → `set_dynamic_demo`
afterwards while the quality rebuild does not.

Reused: static/prop VBO/IBO pairs (surplus pairs deleted); scene, planar and
bloom targets; white/font/decal-atlas/black-cube textures; all programs and
quad VBOs; catalog surface/fixture texture caches; prop model texture cache;
session decoded-PNG cache; all switches (`offscreen_enabled`, bloom,
reflections, culling, indexing, vertex layout, quality, filtering).

Destroyed/replaced: previous `level_textures` (pack uploads); lightmap pages
(cleared then re-uploaded); probe cubemaps (cleared by `set_routing`, re-baked
at the end of `set_level`); static batches, prop draws, `material_textures`,
`materials`, spatial grid, dynamic lighting clone, animations, level stats;
`decal.external`; the temporary CPU `LevelMesh`/prop meshes (dropped after
upload).

Documented flags: the probe bake runs before `set_dynamic_demo` on the menu
path and without any dynamic reset on the rebuild path, so
a probe can bake with the previous level's dynamic objects present
(`Needs runtime confirmation`, §26 Q4). `release_profile_textures` leaves
`materials`/`material_textures`/`surface_state`/`dynamic_meshes` referencing
deleted textures until the same call's `set_level` completes; no draws occur
in that window today, but the ordering is a contract, not an invariant
(§25, §26 Q3).

### 4.7 Shutdown

There are **no `Drop` implementations** anywhere in `src/` (verified by
search). `Renderer` owns the SDL `GLContext` and the `glow::Context`; dropping
the renderer destroys the SDL context, and the platform releases every GL
object with it. No `glDelete*` runs at shutdown. `main` returns after
`bench.finish()`; locals drop in reverse declaration order, with the renderer
dropped while SDL and the window are still alive. The renderer's own textures,
buffers, framebuffers, renderbuffers and programs are never individually
deleted — except where per-level release happens (`set_level`,
`release_profile_textures`, `clear_lightmap_pages`, `clear_probes`,
`ensure_*` recreation), which is documented in §7.

### 4.8 Rendering outside the normal frame loop

| Site | When | GL work |
|---|---|---|
| `StartupResources::create` | process start | program compile/link, texture/buffer creation, base state; no draws |
| `build_level_for_load` | every level load/rebuild | lightmap texture upload, buffer/texture uploads |
| `bake_reflection_probes` | every load with probe points | up to 12 `draw_scene_body` submissions into cubemap faces, clears, FBO/viewport changes; the first world draws of the process (nothing if the level has no probe points) |
| `set_dynamic_demo` → `sync_dynamic_meshes` | after load, demo only | buffer/texture create/upload/delete; no draws |
| `set_texture_filtering` | frame loop when the setting changed | binds every resident texture and re-sets filters; lightmap filters too |
| `PLACES_CAPTURE` | frame loop, after UI, before swap | `glReadPixels` of the default framebuffer, CPU row flip, PNG |
| `PLACES_BENCH_FINISH` | frame loop before swap | `glFinish` |
| `PLACES_DUMP_LIGHTMAPS=1` | level load | none (CPU atlas → PNG) |
| First-frame lazy creation | frame 1 / first bloom frame | scene target, bloom targets, first clears |

The probe bake leaves the probe face FBO bound and a face-sized viewport until
the next `render_scene` re-binds the scene target; nothing reads the
framebuffer in between in the shipped paths. `run_blur` leaves depth test and
blend disabled and no program bound; only resolve/present follow it, and they
re-enable depth test.

---

## 5. Complete render-pass graph

This is the graph as implemented, not a hypothetical one. Evidence is given per
stage; the per-pass state is tabulated in §10.

### 5.1 Normal frame (offscreen path — shipped default)

```text
main.rs — FrameLoop::render_and_present
 │
 ├─ drawable = window.drawable_size(); if empty → skip frame
 ├─ renderer.set_drawable_size(drawable)                       [records only]
 │
 ├─ renderer.render_scene(camera)
 │   │
 │   ├─ ensure_scene_target(target_size)                       [create/resize FBO]
 │   ├─ scene_view_projection(render_size)                     [CPU: mvp + frustum]
 │   ├─ bind_scene_target: bind FBO, viewport=render_size,
 │   │     clear COLOR|DEPTH (0.08, 0.08, 0.09, 1)
 │   │
 │   ├─ [Full only, offscreen] capture_reflections
 │   │   ├─ nearest in-frustum mirror plane (≤1/frame)         [CPU]
 │   │   ├─ ensure_planar (half-res colour + depth)            [create/resize]
 │   │   ├─ bind planar FBO, viewport, clear, front_face(CW)
 │   │   ├─ draw_scene_body(mirrored_mvp, mirrored_frustum, cull)
 │   │   │      (full 6-stage body below; reflection sampling
 │   │   │       suppressed by u_reflect_mode=0; mirror batches skipped)
 │   │   ├─ front_face(CCW)                                    [restore]
 │   │   └─ bind_reflection_textures(planar)                   [units 5/6]
 │   ├─ bind_scene_target AGAIN: bind, viewport, clear         [redundant 2nd clear]
 │   ├─ draw_scene_body(scene_mvp, frustum, cull)
 │   │   1. static batches — Opaque     (World program, depth write)
 │   │   2. prop batches                (World, per-model texture/emission)
 │   │   3. dynamic objects             (World, per-object u_model/u_mvp/u_light_scale)
 │   │   4. static batches — Cutout     (Cutout program, discard)
 │   │   5. translucent batches         (World, BLEND, depth write OFF,
 │   │                                   sorted back-to-front per spatial batch)
 │   │   6. decal batches               (Decal program, polygon offset, depth write)
 │   │
 │   ├─ [bloom enabled && emissive visible && post.blooms()]
 │   │   capture_bloom:
 │   │   ├─ ensure_targets (emissive = scene size + shared scene depth;
 │   │   │                  blur A/B = scene/4, LINEAR)
 │   │   ├─ begin_emissive: bind emissive FBO, viewport=scene size,
 │   │   │       clear COLOR only (black), DEPTH_TEST on, depth_mask OFF
 │   │   ├─ draw_emissive_body: stages 1–5 above with u_emission_only=1,
 │   │   │       NO decals, only emissive batches; depth test against the
 │   │   │       main pass's depth buffer
 │   │   ├─ end_emissive: depth_mask ON
 │   │   └─ finish_bloom: blur emissive→blur_a (x), blur_a→blur_b (y)
 │   │
 │   ├─ disable_scene_attributes; unbind texture unit 0, ARRAY_BUFFER, program
 │   │
 │   └─ resolve_scene:
 │       ├─ post.is_identity() → present_scene: present program, copy quad
 │       │       to default FB, depth test off, clear DEPTH afterwards
 │       └─ else → post.resolve: resolve program reads scene (unit 0) and
 │               bloom (unit 1), applies bloom add + exposure + tone +
 │               grade, writes default FB; then clear DEPTH
 │
 ├─ [settings filtering changed] renderer.set_texture_filtering(...)
 ├─ renderer.render_ui(ui_vertices)
 │     default FB, UI viewport (480x272 scaled), World program,
 │     font atlas as plain surface; depth test OFF, BLEND on;
 │     ortho MVP; lightmaps off; fog off; reflections off
 ├─ [PLACES_BENCH_FINISH] renderer.finish() = glFinish
 ├─ [PLACES_CAPTURE] capture_default_framebuffer() → PNG, stop
 └─ window.gl_swap_window() unless PLACES_BENCH_NOSWAP=1
```

Ordering dependencies:

- The main body writes scene colour and depth; bloom reads the same depth (so
  emitters occluded by walls do not bloom) and the emissive pass must run before
  blur; resolve reads scene colour + bloom.
- The reflection capture runs *before* the main body so that `active_plane` and
  the planar texture exist; it re-runs the whole scene body and leaves the
  tracked material/program caches invalidated; the caller re-binds and re-clears
  the scene target afterwards.
- The decal pass is the only mid-frame switch to the Decal program and the
  only place that invalidates the tracked program/surface caches on exit
  (World ↔ Cutout switches also occur inside the body whenever cutout batches
  exist, in every scene/reflection/probe body); it restores the World
  program and invalidates `current_pass`, `surface_state` and
  `frame_state_valid` (`Renderer::draw_decal_batches`).
- The UI pass runs after resolve, on the default framebuffer, so it bypasses
  tone/grade/bloom entirely.
- The scene target is cleared twice when a planar capture ran; the second clear
  is redundant but harmless.

### 5.2 Direct (no-offscreen) path

Triggered by `PLACES_NO_OFFSCREEN=1`, `offscreen_failed`, or a target-creation
failure. Differences from §5.1:

- `bind_scene_target(false, drawable)` binds the default framebuffer, so the
  scene is drawn at the drawable resolution (Low's reduced scene resolution is
  ignored on this path).
- No `capture_reflections`; `bind_reflection_textures(None)` binds the probe
  cube (or black cube) and the white planar fallback once.
- No `capture_bloom`, no `resolve_scene`, no `present_scene`. The scene colour
  is the window content.
- `render_stats.reflection_passes` stays 0.
- The UI pass still runs afterwards.

This path is documented as a benchmark/diagnostic A/B and is **not
pixel-identical** to the offscreen path.

### 5.3 Reflection probe bake (level load, offscreen)

Runs inside `Renderer::set_level` after textures are resident, before the
level's first frame; up to `MAX_REFLECTION_PROBES = 2`.

```text
bake_reflection_probes
 ├─ bind_reflection_textures(None)     [black cube + white planar; completes samplers]
 ├─ for each wanted probe (≤2):
 │   ├─ ProbeTarget::create(profile, position)
 │   │     position = cluster centroid + 1.2 m Y
 │   │     cube RGBA8, 64 texels Full / 32 Low, CLAMP, LINEAR,
 │   │     FBO + DEPTH_COMPONENT16 renderbuffer
 │   ├─ projection = perspective_rh_gl(90°, 1.0, near 0.1, far 100)
 │   └─ for each of 6 cube faces:
 │       ├─ probe.bind_face(face); viewport = face size; clear COLOR|DEPTH
 │       ├─ view = look_at_rh(eye, eye+face_dir, face_up)
 │       ├─ reflection_capture = true  → u_reflect_mode = 0 for all draws
 │       ├─ draw_scene_body(projection*view, frustum, cull=true)
 │       └─ (all six stages, including decals; no geometric exclusion)
 └─ scene_mvp = IDENTITY (only on the drawn-probe path; next render_scene overwrites)
```

Notes: the bake does not exclude the probing geometry; it excludes all
reflection *sampling*, which prevents reading the incomplete cubemap. Dynamic
objects present at load time are baked into the probes (see §26 Q4). No culling
of back faces anywhere, so mirrored/probe geometry relies on the shader's
`gl_FrontFacing` normal flip.

### 5.4 Planar reflection capture (per frame)

Facts a port must keep: at most one plane per frame; Full profile only;
half-resolution target with its own 16-bit depth; mirrored view-projection is
`mvp * mirror(normal, offset)` with no oblique near-plane clipping; reflection
sampling is suppressed during the capture itself (`u_reflect_mode = 0`); the
capture skips the static batches whose material maps to the captured plane
(`is_capture_mirror` / `is_mirror_range`); the world shader discards planar
samples outside the projected image and far from the plane (`inside`,
`on_plane` in §8.4).

### 5.5 Lightmap upload as its own flow

```text
LevelLighting::bake_with                           (CPU; always runs)
        │
mesh emission + cache lookup by content key         (CPU)
        │   cache hit → skip the atlas fill
        │   cache miss → LightmapAtlas::bake / fill (CPU pages, RGB8) → cache insert
        │
upload_level_lightmaps
 ├─ clear_lightmap_pages (delete old pages → white slots)
 ├─ require pages.len() ≤ LIGHTMAP_PAGE_SLOTS = 2
 ├─ per page: create texture RGB8, CLAMP, no mips, user filter, upload
 └─ failure: delete created pages, white slots, resident=false,
             and (in build_level_for_load) rebuild the whole mesh vertex-lit
```

The runtime binding is `bind_lightmap_units`, called at the start of every
world draw body (`draw_scene_body`, `draw_emissive_body`) and once in
`render_ui`; it binds both slots and restores active unit 0.

### 5.6 Resolve/present variants and fallbacks

| Condition | Path |
|---|---|
| offscreen + `post = None` (program link failed) | `present_scene` copy quad |
| offscreen + `PostSettings::is_identity()` (Low without bloom) | `present_scene` copy quad |
| offscreen + post active | `post.resolve` with scene + bloom (or scene as the zero-strength bloom fallback) |
| direct path | none (scene already on the default framebuffer) |
| scene-target create failed | session falls back to the direct path |
| planar target create failed | planar disabled for the session; probes stay |
| probe create failed | bake aborts at the failing probe; earlier baked probes stay resident and the nearest one is used; the black cube applies only when zero probes were baked |
| bloom target create failed | bloom stage skipped for the frame |

### 5.7 Pass ordering recap (frame)

1. Clear scene (color+depth).
2. Planar reflection capture (Full/offscreen, ≤1 plane) — optional.
3. Re-bind scene, re-clear.
4. Static opaque → props → dynamic → cutout → translucent → decals.
5. Emissive capture (+shared depth) → blur ×2 — optional.
6. Resolve or present copy to default FB; clear depth.
7. UI.
8. Swap.

Dependencies on previous results: step 2 reads nothing from step 1 (it clears
its own target) but writes the planar texture and `active_plane` used by step 4;
step 5 reads the step-4 depth buffer and scene parameters (camera, lightmaps);
step 6 reads step 4's colour and step 5's bloom; step 7 reads only the default
framebuffer binding left by step 6.

---

## 6. OpenGL API inventory

This section summarises the complete GL surface. **No `glow::Context` method
is called outside** `src/render/renderer.rs`, `src/render/framebuffer.rs`,
`src/render/postprocess.rs`, `src/render/reflections.rs` and one helper in
`src/render/view.rs`; `src/render/tests.rs` uses glow only as test data
(`NativeTexture` handles and enum constants) with no live context. `src/main.rs`
separately owns GL **context policy** through SDL (attribute configuration,
context creation, swap interval and buffer swap) and `renderer.rs` creates the
context; no other module touches GL. No GL leaks into tools, tests, the
loader, materials, lighting or the level editor (the editor has its own
separate WebGL1 preview).

Distinct glow methods used (about 60; the list below is complete, the exact
count depends on grouping convention):

- Shader/program: `create_shader`, `shader_source`, `compile_shader`,
  `get_shader_compile_status`, `get_shader_info_log`, `delete_shader`,
  `create_program`, `delete_program`, `attach_shader`, `link_program`,
  `get_program_link_status`, `get_program_info_log`, `use_program`,
  `bind_attrib_location`, `get_attrib_location`.
- Uniforms: `get_uniform_location`, `uniform_1_i32`, `uniform_1_f32`,
  `uniform_2_f32`, `uniform_3_f32`, `uniform_4_f32`,
  `uniform_matrix_4_f32_slice`.
- Textures: `create_texture`, `delete_texture`, `bind_texture`,
  `active_texture`, `tex_image_2d`, `tex_parameter_i32`, `generate_mipmap`.
- Buffers: `create_buffer`, `delete_buffer`, `bind_buffer`,
  `buffer_data_u8_slice`.
- Framebuffers: `create_framebuffer`, `delete_framebuffer`,
  `bind_framebuffer`, `framebuffer_texture_2d`, `framebuffer_renderbuffer`,
  `check_framebuffer_status`.
- Renderbuffers: `create_renderbuffer`, `delete_renderbuffer`,
  `bind_renderbuffer`, `renderbuffer_storage`.
- Readback: `read_pixels`.
- State: `enable`, `disable`, `depth_func`, `depth_mask`, `blend_func`,
  `front_face`, `polygon_offset`, `clear`, `clear_color`, `viewport`.
- Draw: `draw_arrays`, `draw_elements`.
- Vertex attributes: `enable_vertex_attrib_array`,
  `disable_vertex_attrib_array`, `vertex_attrib_pointer_f32`.
- Sync: `finish`.

**Absent (verified):** `glGetError`, `glGetString`/version queries,
`get_parameter` limit queries, extension checks, `pixel_store_i32`, VAO calls,
instancing, `buffer_sub_data`, `tex_sub_image_2d`, `blit_framebuffer`,
`copy_tex_image`, `scissor`, `cull_face`, `color_mask`, `blend_equation`,
`blend_func_separate`, sampler objects, fences, sRGB formats, float formats,
anisotropy, multisampling, stencil, MRT/`draw_buffers`, `read_buffer`.

### 6.1 Inventory by owning subsystem

| API/resource | Source | Owning system | Created/changed when | Used during | Notes |
|---|---|---|---|---|---|
| Context creation, GL attributes | `main.rs — configure_gl_attributes/create_window`; `renderer.rs — Renderer::new` | initialization | startup (attrs set twice) | whole process | GLES 2.0 request, Compatibility 2.1 fallback; no MSAA/sRGB/stencil requested |
| `DEPTH_TEST`, `LEQUAL`, clear color | `renderer.rs — StartupResources::create` | initialization | once | all frames | depth mask/blend/cull left at defaults |
| 6 shader programs + attribute slot verification | `renderer.rs — create_scene_programs/create_present_pass`; `postprocess.rs — PostProcess::create` | initialization | once | scene/present/post | never deleted |
| White sheet, font atlas, decal atlas, black cube, UI VBO | `renderer.rs — StartupResources::create` | fallback/UI/decals | once | everywhere | font/decal atlases are procedural pixels |
| Static/prop VBO/IBO chunk pairs | `renderer.rs — upload_chunks` | world geometry / props | level load | scene bodies (incl. reflections/probes) | ≤65 536 vertices/chunk, u16 indices, STATIC_DRAW; pairs reused across loads |
| UI VBO re-upload | `renderer.rs — render_ui` | UI | every frame | UI | DYNAMIC_DRAW |
| Material/surface textures | `renderer.rs — set_level` upload loop | material textures | level load | world/UI | session (catalog) or per-level (pack) caches |
| Prop/fixture/decal-sheet textures | `renderer.rs — upload_model_textures/upload_fixture_sheets/load_decal_sheets` | material textures, props, fixtures, decals | level load / first model use | scene/decal | clamp+mips (props/fixtures) or repeat+mips (decals) |
| Lightmap pages | `renderer.rs — upload_level_lightmaps/upload_lightmap_page` | lightmaps | level load | world/UI | RGB8, CLAMP, no mips |
| Scene target | `framebuffer.rs — SceneTarget::create` | framebuffer management | first frame / resize / quality change | every offscreen frame | RGBA8 + DEPTH24→16 |
| Bloom targets | `postprocess.rs — ensure_targets` | postprocess | first bloom frame / resize | bloom | emissive shares scene depth; A/B quarter size |
| Planar target | `reflections.rs — PlanarTarget::create` | planar reflections | first active frame / resize | mirror pass | half res, own DEPTH16 |
| Probe cubemaps | `reflections.rs — ProbeTarget::create` | reflection probes | level load | probe bake | 6 faces, own DEPTH16 |
| Reflection bindings | `renderer.rs — bind_reflection_textures` | reflections | per frame / bake | world draws | units 5/6, fallbacks |
| Lightmap bindings | `renderer.rs — bind_lightmap_units` | lightmaps | every body | world/UI | units 2/3, white fallbacks |
| Surface binds/uniforms | `renderer.rs — apply_surface_state` | material textures | per material change | world draws | units 0/1/4, cached |
| Draw calls | `renderer.rs — draw_static_pass/draw_prop_batches/draw_dynamic_objects/draw_translucent_pass/draw_decal_batches/render_ui`; `postprocess.rs — run_blur/resolve`; `present_scene` | scene/decal/UI/post | per frame | — | `draw_elements` for geometry, `draw_arrays` for quads/UI |
| Planar capture draws | `renderer.rs — capture_reflections` | planar reflections | ≤1/frame | — | full scene body, CW winding |
| Probe capture draws | `renderer.rs — bake_reflection_probes` | reflection probes | level load | — | 6 full scene bodies per probe |
| Emissive capture draws | `renderer.rs — draw_emissive_body` | bloom | frames with visible emissives | — | same body minus decals |
| Readback | `renderer.rs — capture_default_framebuffer` | screenshot | `PLACES_CAPTURE` frame | dev | RGBA8, CPU row flip |
| Filtering changes | `renderer.rs — set_texture_filtering` | material textures | settings change | next draws | mutates texture objects in place |
| Resource release | `release_profile_textures`, `set_level`, `clear_lightmap_pages`, `clear_probes`, target `destroy`s | cleanup | quality change / level change / resize / failure | — | see §7 |
| `glFinish` | `renderer.rs — finish` | benchmark | `PLACES_BENCH_FINISH` | dev | |
| Test handles | `render/tests.rs` | tests | compile time | tests | no context |

### 6.2 Notable API-level observations (documented, not fixed)

- No `glGetError`/validation anywhere: state misuse and failed binds are silent.
- No `glPixelStorei`: default 4-byte alignment is assumed and holds for all
  current uploads (RGBA8 always; RGB8 lightmap pages are 1024/512 wide).
- No capability queries: 8 attributes, units 0–6, cubemap ≥64, and
  renderbuffer/depth format support are assumed; SDL requests ES2 core first, so
  sized `RGBA8`/`RGB8` formats and `DEPTH_COMPONENT24` are not core ES2 (the
  16-bit fallback and the direct-path fallback are the safety net).
- No VAO: attribute state is global; `glVertexAttribPointer` captures the
  bound `ARRAY_BUFFER`, which is why pointers are re-issued per chunk
  (`bind_chunk`).
- No culling: `glEnable(GL_CULL_FACE)` never appears; two-sided shading uses
  `gl_FrontFacing`.
- Uploads bind on the currently active unit (usually 0); only
  `upload_lightmap_page` pins unit 0 explicitly.

---

## 7. GPU resource inventory

Lifecycle legend: **startup** = created once in `Renderer::new`;
**session** = lives until process exit or an explicit release; **level** =
created per level load; **frame** = created/recreated lazily per frame;
**transient** = created/destroyed within a pass.

### 7.1 Shader programs

| Program | Owner | Created | Used | Destroyed | Fallback |
|---|---|---|---|---|---|
| World | `Renderer::programs[World]` | startup | all scene bodies, UI | never | startup fails → fatal |
| Cutout | `programs[Cutout]` | startup | cutout batches, emissive cutout | never | a failure deletes only the failed program and aborts startup |
| Decal | `Renderer::decal.program` | startup | decal pass | never | startup failure → fatal |
| Present | `Renderer::present.program` | startup | copy presentation | never | startup failure → fatal |
| Resolve | `PostProcess::resolve_program` | startup | resolve pass | never | post process `None` → copy presentation |
| Bloom blur | `PostProcess::blur_program` | startup | 2 blur passes | never | as above |

All six programs are compiled once; no recompilation or hot reload exists.
Partial-failure leaks (VS on FS compile failure; previous startup objects if a
later startup step fails) are documented in §26.

### 7.2 Textures

| Resource | Owner / cache | Created | Uploaded from | Format / mips / wrap | Fallback | Destroyed |
|---|---|---|---|---|---|---|
| Material albedo / emission mask / normal map | `surface_textures` (Catalog, session) + `level_textures` (Pack, level) + `material_textures` (per-material handle vector) | level load | catalog or pack PNGs, quality-fitted | RGBA8, repeat, mips | white sheet on upload failure; `core:tex_missing` diagnostic on resolution failure | `release_profile_textures` (Catalog), next `set_level` (Pack); never at exit |
| Prop model textures | `prop_textures` keyed by model path | first static/dynamic use | GLB embedded images | RGBA8, CLAMP, mips | white sheet; model upload is all-or-nothing with rollback | `release_profile_textures`; never at exit |
| Fixture face sheets | `fixture_sheet_textures` (Catalog) / `level_textures` (Pack) | level load | catalog/pack fixture PNGs | RGBA8, CLAMP, mips | white per slot | as material textures |
| Decal sheets (external) | `decal_sheet_textures` keyed by path | level load | catalog/pack decal PNGs | RGBA8, repeat, mips | generated atlas / missing diagnostic | `release_profile_textures` only (pack sheets outlive their level — §26) |
| Generated decal atlas | `Renderer::decal.texture` | startup | `decals::generate_decal_atlas()` (procedural) | RGBA8, repeat, mips | is the fallback | never |
| Lightmap pages 0/1 | `lightmap_textures` + `lightmap_pages[2]` | level load | baked `LightmapPage.rgb` | RGB8, CLAMP, **no mips** | white sheet | `clear_lightmap_pages` on next load/failure |
| White sheet | `Renderer::white_texture` | startup | `core:tex_white_01` or embedded PNG | RGBA8, CLAMP, NEAREST, no mips | is the universal fallback | never |
| Font atlas | `Renderer::font_texture` | startup | `font::generate_font_atlas()` (procedural) | RGBA8, CLAMP, NEAREST, no mips | none | never |
| Black cube | `Renderer::black_cube` | startup | procedural 1×1 black, 6 faces | RGBA8 cube, CLAMP, LINEAR | probe fallback | never |
| Probe cubemaps | `Reflections::probes` (≤2) | level load (bake) | rendered faces | RGBA8 cube, CLAMP, LINEAR, 64/32 texels | black cube | `clear_probes` on next load/routing change |
| Planar reflection colour | `Reflections::planar` | first active frame/resize | rendered mirror image | RGBA8, CLAMP, LINEAR, half scene | white sheet (sampler still complete) | recreate on size change |
| Scene colour | `SceneTarget::color` | first frame/resize/quality | rendered scene | RGBA8, CLAMP, NEAREST, scene size | direct-to-default path | target recreate |
| Emissive (bloom source) | `BloomTargets::emissive` | first bloom frame/resize | emissive-only scene draw | RGBA8, CLAMP, NEAREST, scene size | bloom skipped | `release_targets` when bloom off/resize |
| Blur A / B | `BloomTargets::blur_a/blur_b` | same | separable blur | RGBA8, CLAMP, LINEAR, scene/4 | bloom skipped | same |

### 7.3 Buffers

| Buffer | Owner | Contents | Usage | Lifecycle |
|---|---|---|---|---|
| `level_buffers: Vec<(vbo, ibo)>` | `Renderer` | static level chunks ≤65 536 vertices, u16 indices | STATIC_DRAW | reused across loads; surplus pairs deleted when the level shrinks; never at exit |
| `prop_buffers: Vec<(vbo, ibo)>` | `Renderer` | pre-transformed prop instances grouped per (model, primitive, cell) | STATIC_DRAW | same policy |
| `DynamicMeshGpu { vbo, ibo }` per model | `Renderer::dynamic_meshes` | model-space dynamic mesh, u16 indices | STATIC_DRAW | deleted/re-created by `sync_dynamic_meshes` (dynamic spawn only); never at exit |
| `ui_vbo` | `Renderer` | UI vertices | DYNAMIC_DRAW, re-uploaded per frame | never at exit |
| `present.vbo`, post `quad_vbo` | present/post | 6-vertex quad | STATIC_DRAW | never at exit |

All uploads are whole-buffer `buffer_data_u8_slice`; no sub-updates or mapping.
Indices are always `GL_UNSIGNED_SHORT` with byte offsets (`start * 2`), so the
65 536-vertex chunk cap is a hard constraint.

### 7.4 Framebuffers and renderbuffers

| Target | Attachments | Format | Size | Completeness / fallback |
|---|---|---|---|---|
| `SceneTarget` | colour texture + depth renderbuffer | RGBA8 + DEPTH24→DEPTH16 | Full: drawable; Low: ≤480 wide, aspect-preserved | completeness checked after creation (with 24, then 16); failure disables offscreen for the session |
| Bloom emissive | colour texture + **scene target's depth renderbuffer** | RGBA8 | scene size | checks before/after depth attach; failure disables bloom for the frame |
| Bloom blur A/B | colour texture | RGBA8 | scene/4 | as above |
| `PlanarTarget` | colour texture + own depth renderbuffer | RGBA8 + DEPTH16 | scene/2 | failure disables planar for the session |
| `ProbeTarget` | one cube face at a time + own depth renderbuffer | RGBA8 + DEPTH16 | 64/32 per face | one completeness check per probe; failure aborts the bake |
| Default framebuffer | SDL window, requested depth 24 | platform | drawable | n/a |

### 7.5 Ownership summary

`Renderer` owns every GL object except the `glow::Context` (also a field) and
the SDL `GLContext` (also a field, which keeps the context alive). There are no
`Drop` implementations: all startup/session resources are reclaimed only when
SDL destroys the context at process exit. Per-level resources are reclaimed by
`set_level`/release paths; render targets on recreation; lightmap pages on
`clear_lightmap_pages`; probes on `set_routing`.

### 7.6 Required resource semantics for a port

1. Every declared sampler must resolve to a complete resource for every draw
   (white sheet / black cube / white planar fallback), even when the shader
   gates the corresponding term off. This is a hard driver-compatibility rule
   in the current implementation, not an optimization.
2. The scene target doubles as the depth source for the emissive pass; a port
   must preserve depth-based occlusion of bloom emitters.
3. Texture lifetime classes: session (catalog/fallback), per-level (pack),
   per-model (prop), startup singleton (white/font/decal-atlas/black-cube),
   render targets (resize-recreated), lightmaps (per-load), probes (per-load).
4. Filtering/wrap/mips are per-texture state today and must remain switchable at
   runtime without re-uploading pixels (`set_texture_filtering`).
5. The static/prop geometry is pre-transformed; no per-frame CPU geometry work
   exists for static content.

---

## 8. Shader inventory

Six GL programs from two vertex and five fragment sources, all embedded in
`src/render/view.rs`. There are **no external shader files** and no other GLSL
in the game (the level editor's WebGL1 preview is separate, §8.12). Sources are
GLSL ES 1.00 style: `attribute`/`varying`/`gl_FragColor`, `texture2D`,
`textureCube`, `#ifdef GL_ES` + `precision mediump float`; no `#version`
directive, no uniform arrays, no loops, no integer uniforms. The only
compile-time variant is the injected `#define ALPHA_CUTOUT 1`
(`view.rs — fragment_shader_source(cutout: bool)`).

Program → passes:

| # | Program | Vertex | Fragment | Passes |
|---|---|---|---|---|
| P1 | World | `VERTEX_SHADER_SRC` | `fragment_shader_source(false)` | static opaque, props, dynamic, translucent, probe bake, planar capture, HUD, emissive body |
| P2 | Cutout | `VERTEX_SHADER_SRC` | `fragment_shader_source(true)` | static cutout batches, emissive cutout batches |
| P3 | Decal | `VERTEX_SHADER_SRC` | `DECAL_FRAGMENT_SHADER_SRC` | decal pass |
| P4 | Present | `PRESENT_VERTEX_SHADER_SRC` | `PRESENT_FRAGMENT_SHADER_SRC` | copy presentation |
| P5 | Resolve | `PRESENT_VERTEX_SHADER_SRC` | `RESOLVE_FRAGMENT_SHADER_SRC` | resolve |
| P6 | Bloom blur | `PRESENT_VERTEX_SHADER_SRC` | `BLOOM_BLUR_FRAGMENT_SHADER_SRC` | 2 blur passes |

Link-time attribute binding: `create_program` binds all eight scene attribute
names to fixed slots 0–7 before linking, for every program (names not declared
are ignored). `scene_attribute_locations` verifies the world program's slots
and hard-fails startup on mismatch.

### 8.1 Shared scene vertex stage (`VERTEX_SHADER_SRC`)

```glsl
attribute vec3  a_pos;            // 0
attribute vec4  a_color;          // 1
attribute vec2  a_uv;             // 2
attribute vec2  a_lightmap_uv;    // 3
attribute float a_lightmap_page;  // 4
attribute vec3  a_normal;         // 5
attribute vec3  a_tangent;        // 6
attribute float a_handedness;     // 7
uniform mat4 u_mvp;
uniform mat4 u_model;
varying vec4 v_color; varying vec2 v_uv; varying vec2 v_lightmap_uv;
varying float v_lightmap_page; varying vec3 v_world_pos;
varying vec3 v_normal; varying vec3 v_tangent; varying float v_handedness;

void main() {
    v_color = a_color; v_uv = a_uv;
    v_lightmap_uv = a_lightmap_uv; v_lightmap_page = a_lightmap_page;
    v_world_pos = (u_model * vec4(a_pos, 1.0)).xyz;
    v_normal = (u_model * vec4(a_normal, 0.0)).xyz;
    v_tangent = (u_model * vec4(a_tangent, 0.0)).xyz;
    v_handedness = a_handedness;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
```

Attribute semantics, spaces and buffer sources:

| Attribute | Meaning | Space | Source / defaults |
|---|---|---|---|
| `a_pos` | vertex position | static: world; prop: world (pre-transformed); dynamic/UI: model | level/prop/dynamic/UI VBOs |
| `a_color` | RGBA shade (baked light × albedo tint); packed as unorm8; exact as float | — | static: emitter + lightmap planning; props: CPU per-instance lighting; dynamic: albedo/tint; UI: UI color |
| `a_uv` | surface UVs (world-metre tiling for surfaces; sheet UVs for fixtures/props/decals; font UVs for HUD) | [0,n] or [0,1] | emitter / prop loader / decals / UI |
| `a_lightmap_uv` | atlas UV inside the chart, [0,1], u16-normalized in both layouts | atlas page space | static level only; others 0 |
| `a_lightmap_page` | page 0/1 or 255 (`LIGHTMAP_NONE`), plain u8 | — | static level only; others 255 |
| `a_normal` | unit geometric normal | world (static); model (dynamic); default `[0,0,1]` | computed from winding by `compute_surface_frames`; defaults for props/dynamic/UI |
| `a_tangent` | unit UV-u tangent, orthogonalised against the normal | same | same |
| `a_handedness` | ±1 bitangent sign; packed normalized snorm8 (±127), exact float ±1 | — | from UV-v vs cross(normal,tangent) |
| `u_mvp` | view-projection (scene, mirrored capture, probe face, HUD ortho) | clip | frame/pass/object |
| `u_model` | model transform (identity for static/HUD; object transform for dynamic) | world | frame/object |

Migration-sensitive details: normals/tangents are transformed by `u_model`
only (no inverse-transpose; object transforms are rigid). The packed layout
relies on fixed-function normalized-integer expansion: color `Unorm8x4`,
normal/tangent `Snorm8x3`, handedness `Snorm8`, lightmap UV `Unorm16x2`,
lightmap page `Uint8`. The decal program never uploads `u_model`, and its
unused varyings are optimized out; a wgpu pipeline must either supply an
identity model or strip the varyings.

### 8.2 World fragment stage — uniforms

Verbatim declarations (`FRAGMENT_SHADER_BODY`):

```glsl
uniform sampler2D u_texture;      uniform sampler2D u_emission_mask;
uniform sampler2D u_normal_map;   uniform sampler2D u_lightmap0;
uniform sampler2D u_lightmap1;    uniform samplerCube u_probe_map;
uniform sampler2D u_planar_map;
uniform vec3  u_emission_color;   uniform float u_emission_mask_enabled;
uniform float u_emission_vertex;  uniform float u_emission_scale;
uniform float u_lightmap_enabled; uniform vec3  u_light_scale;
uniform float u_response_enabled; uniform float u_normal_enabled;
uniform float u_normal_strength;  uniform vec3  u_specular;
uniform float u_roughness;        uniform float u_opacity;
uniform float u_alpha_cutoff;     uniform vec3  u_camera_pos;
uniform vec3  u_fog_color;        uniform float u_fog_density;
uniform float u_fog_reference_y;  uniform float u_fog_height_gain;
uniform float u_reflect_mode;     uniform vec3  u_reflect_strength;
uniform mat4  u_planar_matrix;    uniform vec4  u_planar_plane;
uniform float u_emission_only;
```

| Uniform | Purpose | Producer | Frequency |
|---|---|---|---|
| `u_texture` | material albedo | `apply_surface_state` (unit 0) | material |
| `u_emission_mask` | emissive mask RGB (alpha ignored) | `apply_surface_state` (unit 1, white when absent) | material |
| `u_normal_map` | tangent-space normal map | `apply_surface_state` (unit 4, white when absent) | material |
| `u_lightmap0/1` | atlas pages | `bind_lightmap_units` (units 2/3) | pass/body |
| `u_emission_color` | `material color × intensity` | `apply_surface_state` | material |
| `u_emission_mask_enabled` | 0/1 mask fetch gate | `apply_surface_state` | material |
| `u_emission_vertex` | 1 ⇒ emission comes from `v_color` and the lit term is multiplied by 0 (fixture faces) | `apply_surface_state` (`EmissionState::vertex`) | material |
| `u_emission_scale` | animated emission multiplier (1.0 normally) | `upload_scene_extras` from `EmissionAnimation::factor` | material/frame |
| `u_lightmap_enabled` | global atlas switch (0 for HUD/no-lightmap levels) | `upload_frame_state` | frame/pass |
| `u_light_scale` | light multiplier: `[1,1,1]` static; dynamic object's baked-light probe | `upload_frame_state`, `set_dynamic_frame_state` | frame/object |
| `u_response_enabled` | master gate for normal map + sheen | `apply_surface_state` (Full only) | material |
| `u_normal_enabled` | normal-map fetch gate | `apply_surface_state` | material |
| `u_normal_strength` | xy scale on decoded normal | `apply_surface_state` | material |
| `u_specular` | sheen color × strength | `apply_surface_state` | material |
| `u_roughness` | `1 − shine`, per-surface override wins | `apply_surface_state` | material |
| `u_opacity` | alpha multiplier | `apply_surface_state` | material |
| `u_alpha_cutoff` | cutout threshold (only used under `ALPHA_CUTOUT`) | `apply_surface_state` | material |
| `u_camera_pos` | view vector + fog distance | `upload_frame_state` | frame/pass |
| `u_fog_color/density/reference_y/height_gain` | fog constants (`FogState::SHIPPED`) | `upload_frame_state`; HUD density 0 | frame/pass |
| `u_reflect_mode` | 0 none, 1 probe cube, 2 planar | `upload_scene_extras` via `reflection_uniforms` | material/frame |
| `u_reflect_strength` | specular × authored reflection strength | `upload_scene_extras` | material |
| `u_planar_matrix` | mirrored view-projection for the active plane | `upload_frame_state` | frame/pass |
| `u_planar_plane` | `(normal.xyz, offset)` of the active plane | `upload_frame_state` | frame/pass |
| `u_emission_only` | 1 during bloom capture | `upload_frame_state` from `draw_emissive_body` | pass |

### 8.3 World fragment stage — exact logic and formulas

```glsl
vec4 tex_color = texture2D(u_texture, v_uv);
vec3 mask = vec3(1.0);
if (u_emission_mask_enabled > 0.5) mask = texture2D(u_emission_mask, v_uv).rgb;
vec3 emission = mix(u_emission_color, v_color.rgb, u_emission_vertex)
              * mask * tex_color.rgb * u_emission_scale;
float alpha = tex_color.a * v_color.a * u_opacity;
#ifdef ALPHA_CUTOUT
    if (alpha < u_alpha_cutoff) discard;
#endif
if (u_emission_only > 0.5) { gl_FragColor = vec4(emission, 1.0); return; }

float lightmap_on = u_lightmap_enabled * (1.0 - step(254.5, v_lightmap_page));
vec3 light = vec3(1.0);
if (lightmap_on > 0.5) {
    vec3 lm = mix(texture2D(u_lightmap0, v_lightmap_uv).rgb,
                  texture2D(u_lightmap1, v_lightmap_uv).rgb,
                  step(0.5, v_lightmap_page));
    light = lm;
}
light *= u_light_scale;

vec3 normal = normalize(v_normal);
if (!gl_FrontFacing) normal = -normal;
if (u_response_enabled > 0.5 && u_normal_enabled > 0.5) {
    vec3 tangent_space = texture2D(u_normal_map, v_uv).xyz * 2.0 - 1.0;
    tangent_space.xy *= u_normal_strength;
    vec3 tangent = normalize(v_tangent - normal * dot(normal, v_tangent));
    vec3 bitangent = cross(normal, tangent) * v_handedness;
    normal = normalize(tangent * tangent_space.x + bitangent * tangent_space.y
                       + normal * tangent_space.z);
}

vec3 view = normalize(u_camera_pos - v_world_pos);
vec3 sheen = vec3(0.0);
if (u_response_enabled > 0.5) {
    float facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    float gloss = 1.0 - u_roughness;
    float grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
    float ahead   = pow(facing,       mix(1.0, 24.0, gloss)) * gloss;
    sheen = u_specular * (grazing * 0.55 + ahead * 0.45) * light;
}

vec3 reflection = vec3(0.0);
if (u_reflect_mode > 0.5) {
    float facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    float fresnel = mix(0.08, 1.0, pow(1.0 - facing, 5.0));
    float gloss = clamp(1.0 - u_roughness, 0.0, 1.0);
    float polish = gloss * gloss;
    float weight = mix(fresnel * 0.35, 1.0, polish);
    vec3 sample_color = vec3(0.0);
    if (u_reflect_mode > 1.5) {                       // planar
        vec4 clip = u_planar_matrix * vec4(v_world_pos, 1.0);
        vec2 uv = clip.xy / max(clip.w, 1.0e-4) * 0.5 + 0.5;
        float blur = u_roughness * 0.035;
        if (blur > 0.002) {
            sample_color += texture2D(u_planar_map, uv + vec2( blur,  blur)).rgb;
            sample_color += texture2D(u_planar_map, uv + vec2(-blur,  blur)).rgb;
            sample_color += texture2D(u_planar_map, uv + vec2( blur, -blur)).rgb;
            sample_color += texture2D(u_planar_map, uv + vec2(-blur, -blur)).rgb;
            sample_color *= 0.25;
        } else sample_color = texture2D(u_planar_map, uv).rgb;
        float inside = step(0.0, uv.x) * step(uv.x, 1.0)
                     * step(0.0, uv.y) * step(uv.y, 1.0);
        float on_plane = 1.0 - smoothstep(0.0, 0.08,
            abs(dot(u_planar_plane.xyz, v_world_pos) + u_planar_plane.w));
        weight *= inside * on_plane;
    } else {                                          // probe cube
        vec3 reflected = reflect(-view, normal);
        vec3 sharp = textureCube(u_probe_map, reflected).rgb;
        if (u_roughness > 0.15) {
            vec3 broad = textureCube(u_probe_map,
                                     normalize(mix(reflected, normal, 0.5))).rgb;
            sharp = mix(sharp, broad, u_roughness);
        }
        sample_color = sharp;
    }
    reflection = u_reflect_strength * weight * sample_color;
}

vec3 lit = tex_color.rgb * v_color.rgb * light * (1.0 - u_emission_vertex);
vec3 color = lit + sheen + reflection + emission;

float distance = length(u_camera_pos - v_world_pos);
float below = max(0.0, u_fog_reference_y - v_world_pos.y);
float density = u_fog_density * (1.0 + u_fog_height_gain * min(below, 12.0));
float fog_amount = density * distance;
fog_amount = 1.0 - exp(-fog_amount * fog_amount);
color = mix(color, u_fog_color, clamp(fog_amount, 0.0, 1.0));
gl_FragColor = vec4(color, alpha);
```

Facts to preserve verbatim in a port:

- Emission always multiplies the albedo `tex_color.rgb`; `v_color.rgb` replaces
  `u_emission_color` when `u_emission_vertex > 0.5`, and the lit term is then
  multiplied by zero.
- `u_alpha_cutoff` is uploaded even for the opaque program where it is unused.
- Lightmap selection is branchless per page via `step(0.5, page)`; page 255
  (only value ≥ 254.5) selects the vertex-lit fallback; `u_lightmap_enabled`
  can be 0 while pages stay bound.
- Normal map decode has no green-channel flip; handedness carries the sign.
  The bitangent is `cross(normal, tangent) * handedness`.
- Sheen is scaled by baked light (`* light`), not by a light direction — there
  are no light vectors in the shader.
- Reflection weight is `mix(fresnel * 0.35, 1.0, polish)` with `polish =
  (1-roughness)²`; planar sampling uses a 4-tap disc only when
  `roughness*0.035 > 0.002`, and is rejected outside `[0,1]²` or farther than
  ~0.08 m from the plane (smoothstep).
- Probe sampling broadens with `mix(reflected, normal, 0.5)` only when
  `u_roughness > 0.15`, mixed by `u_roughness`.
- Fog is exponential-squared with a height gain clamped at 12 m below the
  reference; fog is applied to emission too (the emission term is added before
  fog), which the module comment describes as intentional.
- Composition order is `lit + sheen + reflection + emission`, then fog.

### 8.4 World fragment stage — inputs by feature

- **Material inputs:** albedo texture, emission color/intensity, optional mask,
  normal map + strength, specular, roughness/shine, opacity/alpha mode and
  cutoff, reflection strength/mode, plus per-vertex `v_color`.
- **Lighting inputs:** no light arrays; the only light is the baked lightmap
  sample or the baked per-vertex light multiplied by `u_light_scale`.
- **Shadow inputs:** none in the shader — shadows are baked into the lightmap
  or vertex colors (§15).
- **Lightmap inputs:** two page textures, atlas UV, page selector byte, global
  enable flag, and per-vertex baked light fallback.
- **Reflection inputs:** one probe cubemap plus the current probe selection
  made on the CPU; one planar texture plus its matrix/plane; both weighted by
  material specular and roughness.
- **Fog inputs:** four constants plus the camera position.

### 8.5 P3 — Decal program (`DECAL_FRAGMENT_SHADER_SRC`)

```glsl
uniform sampler2D u_texture; uniform float u_alpha_cutoff;
varying vec4 v_color; varying vec2 v_uv;
void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    if (tex_color.a < u_alpha_cutoff) discard;
    gl_FragColor = tex_color * v_color;
}
```

Vertex stage is the shared scene stage; `u_mvp` is the body MVP, `u_texture` is
unit 0, `u_alpha_cutoff = DECAL_ALPHA_CUTOFF = 0.5`. Blend is off (opaque with
discard), depth test/write on, polygon offset `(-1, -4)`. `v_color` is the CPU
decal tint (surface tint × baked light); decal geometry is lifted
`DECAL_SURFACE_OFFSET_M = 2.0e-4` along the surface normal. External sheets use
both in-plane UV axes swapped relative to the quad corner order because the PNG
row order runs opposite the decal V axis.

### 8.6 P4 — Present program

Vertex: only `a_pos`; `v_uv = a_pos.xy`; `gl_Position = u_mvp * a_pos` with
`present_matrix()` mapping `[0,1]²` to `[-1,1]²` y-up. Fragment: `gl_FragColor
= vec4(texture2D(u_scene, v_uv).rgb, 1.0)`. Sampler unit 0, set once at
creation. No tone, no gamma, no scaling; `NEAREST` scene sampling.

### 8.7 P5 — Resolve program

```glsl
vec3 color = texture2D(u_scene, v_uv).rgb;
if (u_bloom_strength > 0.0) color += texture2D(u_bloom, v_uv).rgb * u_bloom_strength;
color *= u_exposure;
vec3 above = max(color - u_tone_knee, vec3(0.0));
float span = max(1.0 - u_tone_knee, 1.0e-3);
color = min(color, vec3(u_tone_knee)) + span * (above / (above + span));
float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
color = mix(vec3(luma), color, u_grade_saturation);
color = clamp((color - 0.5) * u_grade_contrast + 0.5, 0.0, 1.0);
gl_FragColor = vec4(color, 1.0);
```

Settings: Full `{exposure 1.0, knee 0.75, saturation 1.03, contrast 1.02}`;
Low identity `{1.0, 1.0, 1.0, 1.0}`; bloom strength `0.42` when the player
enables bloom (independent of profile). When the settings are the identity the
renderer uses the plain copy quad instead. With bloom on but no bloom texture
this frame, the scene texture is bound with strength 0 so the sampler is
complete. No sRGB decode/encode anywhere.

### 8.8 P6 — Bloom blur program

```glsl
vec2 step0 = u_texel * 1.0; vec2 step1 = u_texel * 2.0;
vec3 sum = texture2D(u_source, v_uv).rgb * 0.375;
sum += (texture2D(u_source, v_uv + step0).rgb
      + texture2D(u_source, v_uv - step0).rgb) * 0.25;
sum += (texture2D(u_source, v_uv + step1).rgb
      + texture2D(u_source, v_uv - step1).rgb) * 0.0625;
gl_FragColor = vec4(sum, 1.0);
```

`u_texel` is one texel of the source texture along the pass axis. Pass 1:
emissive (scene size, NEAREST) → blur A (scene/4, LINEAR), axis x. Pass 2:
blur A → blur B, axis y. Weights: 0.375 center, 0.125 each ±1, 0.03125 each ±2.

### 8.9 Program variants and gates

Compile-time variants: only `ALPHA_CUTOUT` (world P2). Everything else is a
runtime float gate: `u_emission_mask_enabled`, `u_emission_vertex`,
`u_emission_only`, `u_lightmap_enabled` + page sentinel, `u_response_enabled`,
`u_normal_enabled`, `u_reflect_mode` 0/1/2, `u_roughness` branches,
`u_bloom_strength > 0`, `u_tone_knee` identity. There are **no Low/High shader
sources**.

### 8.10 Fragment outputs, alpha, discard, blending

- One output (`gl_FragColor`), no MRT, no `gl_FragDepth`.
- Alpha = `tex_color.a * v_color.a * u_opacity`; forced to 1.0 in
  emission-only, present, resolve, blur.
- Discard only under `ALPHA_CUTOUT` and in the decal program; the cutout check
  is placed before the emission-only early-out so emitters also discard in the
  bloom capture.
- Blending only in the translucent pass and UI (`SRC_ALPHA`,
  `ONE_MINUS_SRC_ALPHA`, equation default `FUNC_ADD`, straight alpha).
- No back-face culling; `gl_FrontFacing` flips the shading normal.

### 8.11 Shader-side constants (Rust values the shaders assume)

| Constant | Value | Role |
|---|---|---|
| `DECAL_ALPHA_CUTOFF` | 0.5 | decal discard threshold |
| `DECAL_POLYGON_OFFSET` | (-1.0, -4.0) | decal depth bias |
| `DECAL_SURFACE_OFFSET_M` | 2.0e-4 | decal geometry lift |
| `LIGHTMAP_NONE` | 255 | vertex-lit sentinel |
| `LIGHTMAP_PAGE_SLOTS` | 2 | atlas page samplers |
| `MAX_REFLECTION_PROBES` | 2 | probes baked; one bound per frame |
| `PROBE_FACE_TEXELS_FULL/LOW` | 64 / 32 | probe cube face size |
| `BLOOM_SCALE_DIVISOR` | 4 | bloom target divisor |
| `PLANAR_REFLECTION_SCALE_DIVISOR` | 2 | planar target divisor |
| `BLOOM_STRENGTH` | 0.42 | resolve bloom add |
| `SCENE_NEAR_M` / `SCENE_FAR_M` | 0.1 / 100.0 | projection |
| `UI_REFERENCE_WIDTH/HEIGHT` | 480 / 272 | HUD ortho + viewport |
| `FogState::SHIPPED` | color (0.60, 0.63, 0.68), density 0.0095, ref_y 2.0, height_gain 0.045 | fog |
| clear color | (0.08, 0.08, 0.09, 1.0) | scene/planar/probe clears |
| emissive clear | (0, 0, 0, 1) | bloom source clear |

### 8.12 Not part of the game renderer

`level-editor/js/viewport3d.js` embeds one WebGL1 preview program
(`a_pos`/`a_color`/`a_uv`, uniform `u_mvp`, sampler `u_texture`, uniform
`u_alpha`) used by the browser editor. It is not compiled by the game, shares
no code, and is not a wgpu port target. It is noted only so a repository-wide
shader search does not mistake it for a renderer shader.

---

## 9. Vertex formats and geometry

### 9.1 Vertex layouts

Two interchangeable CPU-side layouts feed the same 8-attribute shader
(`src/render/mesh.rs`):

**Packed (`PackedVertex`, stride 36 bytes)** — the default and the shipping
layout; converts `Vertex` → `PackedVertex` at upload time:

| Offset | Field | Type | GL type / normalized |
|---|---|---|---|
| 0 | pos | f32×3 | FLOAT |
| 12 | uv | f32×2 | FLOAT |
| 20 | color | u8×4 | UNSIGNED_BYTE, normalized |
| 24 | lightmap | u16×2 | UNSIGNED_SHORT, normalized |
| 28 | normal | i8×3 | BYTE, normalized (Snorm) |
| 31 | tangent | i8×3 | BYTE, normalized |
| 34 | handedness | i8 | BYTE, normalized; encoded ±1 (−127/127) |
| 35 | lightmap_page | u8 | UNSIGNED_BYTE, **not** normalized |

**Exact (`Vertex`, stride 72 bytes)** — the authoring/build/test layout and a
benchmark switch; every attribute f32 except the two lightmap attributes,
which stay u16-normalized and u8 so packed and exact sample identically:

`pos f32×3`, `color f32×4`, `uv f32×2`, `normal f32×3`, `tangent f32×3`,
`handedness f32`, `lightmap u16×2`, `lightmap_page u8`.

Quantization: `quantize_normal` maps normals/tangents to `[-1,1]→i8` (zero
vector → `[0,0,127]`), `quantize_unit` is the `[0,1] → u8` colour quantiser,
`u16` UVs are normalized by 65535 on the GPU, handedness is
`-127` if negative else `127`. Attribute pointers are described once in
`renderer.rs — scene_attribute_table` and tested against `offset_of!`
assertions. Documentation drift: the `VertexLayout` doc comment claims 24/36
bytes; actual strides are 36/72 (§26).

### 9.2 Geometry sources and batching

| Geometry | Emitted by | Vertex space | Indices | Notes |
|---|---|---|---|---|
| Static world (floors, ceilings, skirt/reveals, walls, thresholds, fixtures, prop fallback boxes) | `render/geometry.rs`, `render/architecture.rs`, `render/fixtures.rs`, orchestrated by `render.rs`/`render/api.rs` | world | u16 chunks | split into `LevelMeshRange`s by `SurfaceKey` (surface kind, material, shine) and spatial cell |
| Props (GLB models) | `render/props.rs` (instancing), packed by `pack_prop_batches` | world (each instance pre-transformed on CPU, per-vertex baked light, tint) | u16 chunks | batches keyed by (model, spatial cell); textures per model; untextured primitives bind white and rely on `baseColorFactor` baked into `v_color` |
| Decals | `render.rs — add_decal_quad` + `render/decals.rs` | world (lifted 2e-4 m) | u16 chunks | decal material slots select generated atlas or external sheets; UVs flipped per sheet orientation |
| Dynamic objects | `render/dynamic.rs` (scene), `renderer.rs — upload_dynamic_mesh` | model (floor-contact origin, +Y up, yaw about Y) | u16 chunks | per-object `u_model`/`u_mvp`, model-space normals/tangents; no lightmap; light via `u_light_scale` probe |
| UI/HUD | `src/ui.rs`, `src/perf.rs` | 480×272 UI pixel space | indexless (`draw_arrays`) | world program with ortho MVP; `LIGHTMAP_NONE`, plain surface state |
| Present/bloom/post quads | `framebuffer.rs — PRESENT_QUAD` | `[0,1]²` | none | positions double as UVs; `present_matrix()` scales to NDC |
| Shadow geometry | **N/A — not implemented.** No shadow-map passes, no caster lists, no shadow VBOs. | — | — | baking supplies shadows (§15) |
| Debug geometry | **N/A as a distinct renderer path.** The perf overlay is ordinary UI geometry; there is no wireframe/gizmo path. | — | — | |

### 9.3 Chunking and index format

- All scene geometry uses `GL_UNSIGNED_SHORT` indices with byte offset
  `index_start * 2`; there is no base-vertex support, so a chunk must contain
  ≤65 536 vertices (`spatial::MAX_INDEX_VERTICES`).
- `MeshPacker` splits material runs into chunks, re-basing indices per chunk,
  and preserves a spatial-cell partition inside the mesh so frustum culling
  works per batch.
- Buffer reuse: `upload_chunks` reuses existing VBO/IBO pairs while the chunk
  count matches, deletes surplus pairs when a level needs fewer, and never
  reallocates a pair in place (whole-buffer `buffer_data_u8_slice`).
- Batch order in the mesh is surface-family order (floor, ceiling, wall, light,
  prop fallback) × material × spatial cell; draw order additionally follows the
  pass grouping in §5.1.

### 9.4 Transforms and bounds

- Static geometry is pre-transformed to world space on the CPU; the scene
  `u_model` is identity.
- Props are pre-transformed per instance on the CPU; per-vertex lighting is
  baked in at pack time (`props.rs`), so prop batches need no model matrix.
- Dynamic objects keep model-space geometry and get a per-object model matrix;
  their light comes from a CPU probe of the static bake at the object centre
  (`DynamicObject::light_scale`, `LevelLighting::sample`).
- Frustum culling is CPU-side per batch against `Aabb` bounds
  (`spatial::Frustum` with `DepthRange::NegativeOneToOne`); culling is enabled
  by default and can be disabled by the benchmark.
- No instancing: one draw call per batch/submesh range, not per instance.

---

## 10. GPU state inventory

### 10.1 Startup state

`StartupResources::create` establishes only:

- `GL_DEPTH_TEST` enabled;
- `glDepthFunc(GL_LEQUAL)` (never changed later);
- `glClearColor(0.08, 0.08, 0.09, 1.0)`.

Everything else is the GL default and relied upon: depth write on, front face
CCW, cull disabled, blend disabled with `(ONE, ZERO)` and `FUNC_ADD`, color
mask all-on, polygon offset `(0,0)`/disabled, scissor disabled, no VAO, pixel
store alignment 4, clear depth 1.0.

### 10.2 Per-pass state table

D = depth test, W = depth write, F = depth func, C = cull, FF = front face,
B = blend, PO = polygon offset, CM = color mask.

| # | Pass | D | W | F | C | FF | Blend | PO | CM | Target | Viewport | Program |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Scene clear | on | on | LEQUAL | off | CCW | off | off | all | scene FBO or default | render size | swapped in by first pass |
| 2 | Static opaque / props / dynamic | on | on | LEQUAL | off | CCW | off | off | all | scene / planar / probe | target size | World |
| 3 | Static cutout | on | on | LEQUAL | off | CCW | off | off | all | same | same | Cutout (discard) |
| 4 | Translucent | on | **off** | LEQUAL | off | CCW | **on** SRC_ALPHA / ONE_MINUS_SRC_ALPHA, FUNC_ADD | off | all | same | same | World |
| 5 | Decals | on | on | LEQUAL | off | CCW | off | **on (-1, -4)** | all | same | same | Decal |
| 6 | Planar capture body | per sub-pass | per sub-pass | LEQUAL | off | **CW** | per sub-pass | per sub-pass | all | planar FBO (half) | half scene | World/Cutout/Decal |
| 7 | Probe face body | per sub-pass | per sub-pass | LEQUAL | off | CCW | per sub-pass | per sub-pass | all | probe face FBO | face size | World/Cutout/Decal |
| 8 | Emissive body | on | **off** | LEQUAL | off | CCW | off (translucent sub-pass blends) | off | all | emissive FBO (+scene depth) | scene size | World/Cutout, `u_emission_only=1` |
| 9 | Bloom blur ×2 | **off** | (irrelevant) | LEQUAL | off | CCW | off | off | all | blur A/B FBOs | quarter | Blur |
| 10 | Resolve | **off** | on | LEQUAL | off | CCW | off | off | all | default FB | drawable | Resolve |
| 11 | Present copy | **off** | on | LEQUAL | off | CCW | off | off | all | default FB | drawable | Present |
| 12 | UI | **off** | on | LEQUAL | off | CCW | **on** SRC_ALPHA / ONE_MINUS_SRC_ALPHA | off | all | default FB | centred 480:272 | World (plain) |

Culling is never enabled anywhere; `set_culling` is CPU frustum culling only.
`glCullFace`, `glColorMask`, `glBlendEquation`, `glScissor`, `glStencil*`,
`glLineWidth` and `glSampleCoverage` are never called.

### 10.3 Texture and sampler state

| Texture class | Wrap | Min / mag filter | Mips |
|---|---|---|---|
| Tiling surfaces, generated decal atlas, external sheets | REPEAT | user setting (linear: LINEAR_MIPMAP_LINEAR/LINEAR; nearest: NEAREST_MIPMAP_NEAREST/NEAREST) | generated |
| Fitted prop/fixture sheets | CLAMP_TO_EDGE | same user setting | generated |
| White sheet, font atlas | CLAMP_TO_EDGE | NEAREST / NEAREST | none |
| Lightmap pages | CLAMP_TO_EDGE | user setting, no mip filter | none |
| Black cube, probe cubes | CLAMP_TO_EDGE | LINEAR / LINEAR (cube R unset) | none |
| Scene / emissive targets | CLAMP_TO_EDGE | NEAREST | none |
| Blur A/B, planar target | CLAMP_TO_EDGE | LINEAR | none |

There are no sampler objects and no anisotropy; filtering is a property of each
texture object, mutated in place by `set_texture_filtering` for the tiling,
fitted, decal and lightmap classes (white/font/black-cube/targets are excluded;
probe cubes are only ever created LINEAR). The user switch is a string
(`"linear"` default vs `"nearest"`) compared in the renderer.

### 10.4 Clears

| Site | Target | Bits | Color | Notes |
|---|---|---|---|---|
| `bind_scene_target` | scene FBO or default | COLOR+DEPTH | 0.08/0.08/0.09/1 | called at frame start and again after the planar capture |
| `capture_reflections` | planar FBO | COLOR+DEPTH | scene clear color | sets clear color explicitly first |
| `bake_reflection_probes` | probe face | COLOR+DEPTH | scene clear color | per face |
| `begin_emissive` | emissive FBO | COLOR only | (0,0,0,1), then restores the scene clear color | depth deliberately kept from the main pass |
| `present_scene` | default | DEPTH | — | after the copy |
| `resolve_scene` | default | DEPTH | — | after the resolve |

No clear is ever skipped or scissored; color mask is untouched; the depth mask
is true on every clear path.

### 10.5 Pixel store

No `glPixelStorei` anywhere. Defaults are relied on:
`UNPACK_ALIGNMENT = PACK_ALIGNMENT = 4`. RGBA8 uploads and the RGBA8 readback
are aligned; RGB8 lightmap pages are 512/1024 texels wide, so rows are
multiples of 4. A future page width not divisible by 4 would corrupt uploads
(§26).

### 10.6 Implicit state dependencies (migration-sensitive)

These are places where a pass relies on state established earlier rather than
setting what it needs. They are listed because wgpu makes each one explicit.

1. **Program assumed bound.** `apply_surface_state` and
   `upload_scene_extras` write uniforms and bind textures without binding a
   program; every caller must be running a scene program. Documented at the
   function, not enforced.
2. **Prop and decal texture binds assume active unit 0.**
   `draw_prop_batches` and `draw_decal_batches` call `bind_texture` without
   `active_texture`; correctness depends on `apply_surface_state` /
   `bind_lightmap_units` / `bind_reflection_textures` having restored unit 0.
3. **Reflection units 5/6 are not rebound by scene bodies.**
   `draw_scene_body` binds only lightmaps; reflection textures are established
   once per frame by `bind_reflection_textures`. The planar capture inside
   `capture_reflections` draws the scene body *before* its own final bind, so
   on a level with planar routing and no probe points, unit 5/6 can be empty
   during that first draw (driver-visible "unloadable sampler"; reads are gated
   off). Same class after a planar-target recreation on resize.
4. **Decal program exit restores the World program and resets caches**, but
   nothing else: blend/depth state was never changed by the decal pass, and the
   decal sheet stays bound on the active unit.
5. **`run_blur` leaves depth test and blend disabled and no program bound**;
   only its caller sequence (blur → resolve/present) re-establishes depth test.
6. **`resolve` leaves unit 0 bound to the scene texture** (it unbinds unit 1
   while unit 1 is active, then selects unit 0).
7. **`render_ui` leaves the UI viewport set and attributes 3–7 enabled**, with
   pointers into the UI VBO; the next frame's `bind_scene_target` and
   `bind_chunk` reset viewport and pointers.
8. **Scene target deletion can empty a unit**: `ensure_scene_target` destroys
   the old colour texture that unit 0 may still reference; the next
   `apply_surface_state` rebinds.
9. **`set_texture_filtering` unbinds unit 0 and does not invalidate the surface
   cache**; the shipped frame order (`render_scene` → filtering → `render_ui`)
   revalidates before the next texture sample.
10. **Emissive pass inherits depth mask/blend from the main body** but
    explicitly sets depth write off and re-enables depth test.
11. **Bloom emissive FBO shares the scene target's depth renderbuffer**; a
    resize deletes that renderbuffer while it remains attached to the emissive
    FBO until the next `ensure_targets`/`release_targets`.
12. **`capture_default_framebuffer` assumes the default framebuffer is bound**
    (true on every shipped path) and contains the resolved image.
13. **`set_lightmap_page` only updates the CPU-side slot array**; the GPU
    binding happens at the next `bind_lightmap_units`.
14. **`set_lightmaps_enabled`/`set_lightmaps_resident` only invalidate
    `frame_state_valid`**; uploads happen at the next `begin_pass`.
    `Renderer::set_light_scale` is dead public API: it assigns
    `self.light_scale` only, does not invalidate the frame cache, and has no
    call sites anywhere (dynamic objects upload their probe value directly in
    `set_dynamic_frame_state`).
15. **Animation uniform staleness:** `u_emission_scale` is uploaded in
    `upload_scene_extras` only when the surface state changes; the surface
    cache survives across frames until a program switch/decal/UI/quality event.
    On a level with no decals and no reflections, a material whose state never
    changes could hold a stale animation factor. `Needs runtime confirmation`
    (the shipped demo has decals, so the cache is invalidated every frame).

### 10.7 sRGB / gamma behavior

There is **no sRGB handling anywhere**: no `GL_FRAMEBUFFER_SRGB`, no sRGB
internal formats, no shader transfer function. All uploads and targets are
`RGBA8`/`RGB8` UNORM, sampled raw, and the only tone/exposure/grade step is the
resolve shader. The UI is drawn after resolve and is not graded. This is
documented as deliberate in the README/asset specification. A wgpu surface
format choice that applies sRGB encoding would change the image; blending
would also change because it currently operates on display-space values.

---

## 11. Texture and sampler system

### 11.1 Creation and upload helpers

- `create_texture_2d(gl, w, h, pixels, repeat, linear)` (`renderer.rs`):
  RGBA8 `tex_image_2d`; `repeat` → REPEAT + user mip filter + `generate_mipmap`;
  else CLAMP_TO_EDGE + NEAREST, no mips. Does not select a unit.
- `upload_fitted_texture` (props, fixture faces): RGBA8, CLAMP_TO_EDGE,
  `generate_mipmap`, user filter. Name is historical (it always
  mip-filters despite clamping).
- `upload_lightmap_page`: RGB8 `tex_image_2d`, CLAMP_TO_EDGE, no mips, user
  filter, uploaded while unit 0 is active by design.
- `set_repeat_filter` / `set_lightmap_filter`: apply the two-state user filter
  to an existing texture without re-uploading pixels.
- All textures are decoded once from PNG/GLB to an 8-bit RGBA `RawImage` on the
  CPU, fitted once at upload time to the quality budget through
  `quality::fit_image` / `RawImage::downscaled_to`, and never rescaled per
  frame. Decoded images are cached per session (`LevelManager::TextureCache`,
  `PropAssets`) and per GPU class.

### 11.2 Sources and fallbacks

| Source | Path | Fallback |
|---|---|---|
| Shipped catalog material | `assets/catalog.json` → texture PNG | `core:tex_missing` diagnostic pattern; if that fails to upload, the white sheet |
| Level pack material | pack zip PNG | white sheet on upload failure |
| Prop GLB image | embedded in the GLB | a failed model upload removes the whole model from the scene (all-or-nothing with rollback); a primitive with no authored texture slot draws the white sheet |
| Fixture face | catalog/pack PNG | white sheet per fixture slot |
| External decal sheet | catalog/pack PNG | generated decal atlas or missing pattern |
| Lightmap page | baked atlas RGB | white sheet |
| White sheet | `core:tex_white_01` PNG, embedded copy if the asset root is unavailable | is the fallback |
| Font atlas | procedural | none |
| Missing-material pattern | procedural magenta/black checker 64×64 | is the diagnostic |

Resolution fallbacks, precisely:

- A material that declares a texture (albedo, emission mask or normal map) that
  cannot be resolved falls back to the shared `core:tex_missing` diagnostic
  pattern and clears emission/response/alpha/reflection — **including** a
  declared-but-unresolvable mask or normal map.
- A material that does **not** author an emission mask or normal map resolves
  that slot to `None`; the renderer binds the white sheet and sets
  `u_emission_mask_enabled`/`u_normal_enabled = 0`, so the term is gated off
  without degrading the material.
- An albedo texture that resolves but fails to upload binds the white sheet
  with a warning (the material is not rebuilt).
- A prop model whose texture upload fails as a whole is skipped entirely (all
  of its GPU textures are rolled back); primitives without a texture slot use
  white albedo and bake their `baseColorFactor` into the vertex colour.

### 11.3 Texture-unit helpers and constants

`src/render/view.rs` defines:

| Constant | Value | Purpose |
|---|---|---|
| `SCENE_TEXTURE_UNIT` | 0 | albedo / UI font / present / resolve scene / blur source / decal |
| `EMISSION_MASK_TEXTURE_UNIT` | 1 | world emission mask |
| `LIGHTMAP_TEXTURE_UNIT` / `_1` | 2 / 3 | atlas pages |
| `NORMAL_MAP_TEXTURE_UNIT` | 4 | normal map |
| `REFLECTION_PROBE_TEXTURE_UNIT` | 5 | probe cubemap |
| `REFLECTION_PLANAR_TEXTURE_UNIT` | 6 | planar reflection |
| `BLOOM_TEXTURE_UNIT` | 1 | resolve bloom (same number as the world mask, different program) |
| `PRESENT_TEXTURE_UNIT` | 0 | present scene copy |
| `LIGHTMAP_PAGE_SLOTS` | 2 | page count |

`texture_unit(unit) = glow::TEXTURE0 + unit`. Sampler uniforms are assigned
once at startup by `ProgramUniforms::bind_samplers` (world/cutout), at creation
for present, and at each draw for resolve/blur; decal and blur use literal
`0`.

### 11.4 Unit binding lifecycle in a frame

1. `ensure_scene_target` may delete the texture unit 0 last held.
2. `bind_reflection_textures` binds units 5/6 once (direct path or after the
   planar capture).
3. `draw_scene_body` calls `bind_lightmap_units` (units 2/3, restore 0), then
   the static pass binds lightmaps and, on the first material,
   `apply_surface_state` binds units 0/1/4 (0 restored on exit) for each change.
4. Props bind their texture directly (unit 0, relying on the restore) before
   applying a plain surface state.
5. Decals switch program, set `u_texture=0`, and bind sheets on unit 0.
6. Emissive body repeats step 3 in the emissive target.
7. Blur binds unit 0 (source) and leaves it unbound.
8. Resolve binds scene unit 0 and bloom unit 1, unbinds unit 1 while active,
   and leaves unit 0 selected with the scene texture bound.
9. UI binds lightmaps (2/3) then the font on unit 0 via a plain surface state;
   exits with unit 0's 2D binding cleared.
10. Frame cleanup (`render_scene` tail) unbinds unit 0's texture, the array
    buffer and the program.

### 11.5 Failure and fallback behavior relevant to samplers

- Every declared sampler must always have a complete texture bound (white
  sheet, black cube, white planar, scene-as-bloom), because the shaders declare
  all samplers unconditionally and the Apple GL driver reports incomplete
  sampler units even when the read is gated off. This invariant is tested
  (`render/tests.rs — the_lightmap_units_cover_every_declared_atlas_sampler`
  and related tests).
- `upload_level_lightmaps` failure deletes partial pages and forces a
  vertex-lit rebuild, never a half-bound atlas.
- `release_profile_textures` deletes GPU textures but only while the caller
  immediately rebuilds the level; the ordering is a contract (§26).

---

## 12. Texture-unit binding map

This is the authoritative numeric-unit → semantic mapping, derived from the
constants, the sampler uniforms, and the bind sites. **The semantics, not the
numbers, are the contract.** Units are shared across programs: unit 1 is the
world's emission mask in the world program and the bloom image in the resolve
program; unit 0 carries six different resources across phases.

| Unit | Target | Program(s) | Sampler(s) | Semantic resource | Owner | Source | Fallback | Established by |
|---:|---|---|---|---|---|---|---|---|
| 0 | 2D | World/Cutout | `u_texture` | material albedo | `material_textures` / prop textures / white | catalog/pack PNG / GLB image | white sheet | `apply_surface_state` (cached per material); props bind directly |
| 0 | 2D | Decal | `u_texture` | decal sheet (generated atlas, external PNG, or missing pattern) | `decal.texture` / `decal_sheet_textures` | generated / catalog/pack PNG | generated atlas | `draw_decal_batches` (no unit select) |
| 0 | 2D | World (UI) | `u_texture` | font atlas | `font_texture` | procedural | none | `render_ui` via `apply_surface_state(SurfaceState::plain(font))` |
| 0 | 2D | Present | `u_scene` | scene colour | `SceneTarget::color` | rendered scene | none (only used offscreen) | `present_scene` |
| 0 | 2D | Resolve | `u_scene` | scene colour | `SceneTarget::color` | rendered scene | none | `PostProcess::resolve` |
| 0 | 2D | Blur | `u_source` | blur source (emissive or blur A) | bloom targets | emissive capture / previous blur | none | `run_blur` |
| 1 | 2D | World/Cutout | `u_emission_mask` | material emission mask (RGB; alpha ignored) | `material_textures` / prop textures / white | catalog/pack PNG | white sheet | `apply_surface_state` when mask changes |
| 1 | 2D | Resolve | `u_bloom` | blurred emissive image, strength 0 when unavailable | `BloomTargets::blur_b` | blur pass | scene texture with strength 0 | `PostProcess::resolve` |
| 2 | 2D | World/Cutout | `u_lightmap0` | lightmap atlas page 0 | `lightmap_pages[0]` | baked atlas RGB8 | white sheet | `bind_lightmap_units` every body |
| 3 | 2D | World/Cutout | `u_lightmap1` | lightmap atlas page 1 | `lightmap_pages[1]` | baked atlas RGB8 | white sheet | `bind_lightmap_units` every body |
| 4 | 2D | World/Cutout | `u_normal_map` | tangent-space normal map | `material_textures` / prop textures / white | catalog/pack PNG | white sheet | `apply_surface_state` when normal changes |
| 5 | CUBE | World/Cutout | `u_probe_map` | static reflection probe chosen nearest the camera | `Reflections::probes` / `black_cube` | 6 rendered faces | 1×1 black cube | `bind_reflection_textures` once per frame / bake start |
| 6 | 2D | World/Cutout | `u_planar_map` | current frame's planar reflection image | `Reflections::planar` / white sheet | mirrored scene pass | white sheet | `bind_reflection_textures` once per frame / after capture |

Programs that only use a subset carry unused sampler declarations for the rest;
the renderer keeps every declared sampler complete anyway.

### 12.1 Semantic conversion table (for Stage 4+)

| Current GL concept | Semantic meaning | Future backend should represent as |
|---|---|---|
| Unit 0 world albedo | batch material base-colour image, sampled with surface UVs | per-material base-colour resource (per-surface family wrap/filter contract) |
| Unit 0 UI font | HUD glyph atlas drawn unlit | UI font atlas resource + a plain UI material/pipeline |
| Unit 0 present/resolve scene | scene colour image | scene colour render-target view |
| Unit 0 blur source | transient blur input | transient bloom-resize source resource |
| Unit 1 world mask | optional per-material emissive mask gate | optional emissive mask resource + a shared neutral white default; gated by a material flag |
| Unit 1 resolve bloom | blurred emissive addition | resolve input bloom resource with a strength of zero when unavailable |
| Units 2/3 pages | ≤2 lightmap atlas pages + per-vertex page selector | array/list of page textures + page index vertex attribute; every slot defined (white when absent) |
| Unit 4 normal | optional tangent-space normal map | optional normal-map resource + neutral tangent-space default + strength parameter |
| Unit 5 probe | static probe cubemap (nearest to camera) | probe cube resource + black-cube default; selection is a frame-level resource choice |
| Unit 6 planar | single active planar reflection image | planar reflection colour resource + weight/plane uniforms + white default |
| White sheet | shared neutral fallback (opaque white, unlit) | shared white default resource with a dedicated sampler policy |
| Black cube | neutral fallback cubemap | shared black cube default |
| `LIGHTMAP_NONE` page byte | per-vertex "no atlas chart" sentinel | page selector value meaning "vertex-light only" + a global lightmap enable flag |
| `set_texture_filtering` | user filter preference across texture classes | sampler-state policy keyed by texture class; changing it must not require re-upload |
| `release_profile_textures` | GPU texture-cache invalidation on quality change | explicit resource-generation invalidation; CPU decoded images remain cached |
| `TextureOrigin::{Catalog, Pack, Missing}` | lifetime classes: session / per-level / shared fallback | resource lifetime tags (session cache, level scope, fallback) |
| `MaterialTable` texture indices | CPU-side identity mapping material → texture slot | material descriptors referencing texture resources; indices stay CPU-side |

---

## 13. Material rendering

### 13.1 The rendering-facing material model

Materials are authored as catalog entries plus per-level references. The CPU
pipeline (`materials::resolve_materials`) produces a `MaterialTable`:

- per distinct texture, one `ResolvedTexture { origin, class, image }` where
  `origin` is Catalog (session upload), Pack (per-level upload) or Missing
  (diagnostic), and `class` is the quality texture class;
- per material: albedo `texture_index`, optional emission
  (`color × intensity`, mask texture index, animated effect), optional
  response (normal texture index, `normal_strength`, `specular`, shine-derived
  roughness), alpha mode (opaque / cutout / blend with cutoff and opacity), and
  optional reflection (mode + strength).

The renderer mirrors this into `MaterialRenderState` (texture slots, emissions,
responses, alphas, reflections) plus a parallel `material_textures:
Vec<glow::Texture>`.

### 13.2 Per-surface vs per-material

Geometry is keyed by `SurfaceKey` = (surface kind, material index, optional
shine override). The renderer resolves a `SurfaceState` per key:

- albedo, emission (vertex or uniform, mask), normal map + strength,
  specular/sheen, roughness (from shine), opacity/alpha mode, reflection mode
  + strength.

A per-surface shine override changes the key, so two surfaces sharing a
material but not a shine draw in separate batches with different roughness.

### 13.3 Fallback behavior

| Situation | Result |
|---|---|
| Unknown material id / missing PNG / missing catalog texture | shared diagnostic `core:tex_missing` checker; emission/response/alpha/reflection cleared |
| Albedo texture upload failure | white sheet, one warning |
| No emission mask | white mask, `u_emission_mask_enabled = 0` |
| No normal map | white normal map, `u_normal_enabled = 0` |
| No reflection | `u_reflect_mode = 0` |
| Full profile | response (normal + sheen) drawn |
| Low profile | `u_response_enabled = 0`; the normal unit binds the white sheet (the authored normal map stays resident but is not bound), and an authored emission mask is still bound and used |
| Prop primitive without texture | white albedo, base colour factor baked into vertex colour |
| Fixture without a face sheet | white sheet per fixture slot |

### 13.4 What the shader does with the material (see §8.3)

`lit = albedo × v_color × light × (1 − emission_vertex)`;
`emission = (uniform color | vertex color) × mask × albedo × animation scale`;
`alpha = albedo.a × v_color.a × opacity`, cut with the material cutoff in the
cutout program; sheen and reflection are added only in the Full profile and
only when the material authors them.

### 13.5 Rendering-facing parameter authority

| Parameter | Source of truth | Consumer |
|---|---|---|
| Texture ids and classes | catalog + level material resolution | uploader |
| Emission color/intensity/mask/animation | material table + level `animated_emissions` | shader uniforms |
| Shine → roughness | material or per-level shine override; `roughness = 1 − shine` | `u_roughness` |
| Normal strength (default 1.0, max 2.0) | `MaterialResponse` | `u_normal_strength` |
| Alpha mode/cutoff/opacity (default cutoff 0.5) | `MaterialAlpha` | `u_alpha_cutoff`/`u_opacity`/pass selection |
| Reflection mode/strength | `MaterialReflection` | `u_reflect_mode`/`u_reflect_strength` + routing |

---

## 14. Lighting

### 14.1 What exists

Places has **baked lighting only**: no realtime lights, no light loops, no
light uniform arrays, no dynamic shadows. The lighting system
(`src/lighting/**`) runs on the CPU at level load:

- `LevelLighting::bake_with(level, profile.bake_config())` computes, for every
  static surface sample, a light value from rooms/sources/visibility/occlusion;
- the result is stored either as a lightmap atlas (per-texel) or as per-vertex
  colors (the historical fallback);
- props receive per-instance, per-vertex baked light at pack time;
- dynamic objects sample the static bake with a single point probe at their
  centre (`LevelLighting::sample` → `u_light_scale`);
- fixture luminous faces express their own emission through vertex color and
  `u_emission_vertex`.

### 14.2 Shader consumption

- Lightmap path: `light = texture(lightmap0/1, atlas_uv) * u_light_scale`.
- Vertex path: `light = 1 * u_light_scale` and the baked light already lives in
  `v_color`.
- `lit = albedo × v_color × light × (1 − emission_vertex)`.
- Sheen is multiplied by the same baked `light` (no light direction exists).
- `u_light_scale` is `[1,1,1]` for static geometry and the dynamic object's
  probe value for dynamic objects; `Renderer::set_light_scale` is dead public
  API (no call sites; it does not invalidate the frame cache), so the probe
  value always arrives through `set_dynamic_frame_state`.

### 14.3 Attenuation, intensity, color, ambient

All of these live in the CPU bake and are documented by the lighting code, not
by a shader:

- light sources carry color and intensity; rooms carry baseline/ambient terms;
- attenuation, range-limited falloff, fixture pools, prop contact occlusion and
  wall/room visibility are evaluated per lightmap texel or per vertex;
- the bake writes RGB values into an RGB8 atlas (Full: 16 texels/m on up to two
  1024-px pages; Low: 9 texels/m on two 512-px pages) or into `v_color`;
- a lightmap bake that cannot fit the page budget falls back to the vertex-lit
  mesh **using the same profile lighting bake** (the profile's taps/occlusion
  cell are kept; `BakeConfig::HARD` is selected only for an explicit
  `LightmapMode::Off` build, including the renderer's upload-failure rebuild).

There is no separate ambient uniform; the ambient/baseline contribution is part
of the baked value. Emission is a material term, not a light.

### 14.4 Low/High differences

| Aspect | Full | Low |
|---|---|---|
| Atlas density | 16 texels/m | 9 texels/m |
| Page size | 1024 | 512 |
| Page padding | 2 | 1 |
| Shadow/penumbra taps per axis | 2 (quincunx) | 1 (hard centre) |
| Prop occlusion cell | 0.075 m | 0.15 m |
| Surface response (normal + sheen) | drawn | `u_response_enabled = 0` |

Both profiles bake from the same patch set and the same chart-span cap, so the
geometry splits identically. Lightmap toggling is a build-time decision: a
lightmapped mesh deliberately omits baked light from vertex colors, so turning
lightmaps off triggers a full rebuild with `BakeConfig::HARD`.

---

## 15. Shadows

**There are no shadow maps, shadow framebuffers, shadow shaders, caster
collection, or per-light shadow passes in the current renderer.** No
`shadow`-named GL resource exists. (The word appears in the lighting bake as
the occlusion/penumbra tap count and in the deferred soft-shadow comments; it
never reaches the GPU as a map.)

What exists instead, and must be reproduced:

| Feature | How it works | Quality dependence |
|---|---|---|
| Static-surface shadows | baked into the lightmap / vertex colors by `lighting::bake` with visibility/occlusion tests | Full: quincunx (2 taps/axis); Low: 1 tap |
| Prop shadows on static surfaces | prop occlusion volumes (boxes derived per model at a quality-dependent cell size) participate in the bake | Full: 0.075 m cells; Low: 0.15 m |
| Dynamic objects | no shadows or self-occlusion; lit by one probe of the static bake | none |
| Emitter penumbra | the tap count averages a fixture's visibility over its emitting rectangle | Full penumbra, Low hard edge |
| Reflections/probe bakes | the bake runs the full scene body, so baked lighting in probe faces matches the level | probe face size only |

A wgpu port does **not** need a shadow pipeline for parity; it may optionally
add realtime shadows later, which would be a visual change, not parity.

**Stage 8 status:** this is delivered. The wgpu level build is the historical
vertex-lit bake, so the static-surface shadows and the prop occlusion volumes
reach the GPU inside the uploaded vertex colours, exactly as they do for
`PLACES_NO_LIGHTMAPS=1` in the reference. The per-texel resolution and the soft
penumbra arrive with the lightmap atlas (Stage 9). See
[WGPU_LIGHTING.md](WGPU_LIGHTING.md).

---

## 16. Lightmaps

### 16.1 Creation and caching

- Decision: `LightmapMode::On|Off` from the player setting
  (`PLACES_NO_LIGHTMAPS` override).
- Bake: `lighting::lightmap` plans charts (`LightmapPlan`), allocates them into
  up to `LIGHTMAP_ATLAS_MAX_PAGES = 2` pages, and fills RGB texels from the
  lighting simulation; failures return a `LightmapFailure` reason and the mesh
  is rebuilt vertex-lit.
- Cache: process memory plus an on-disk cache under `cache/lightmaps` (state
  root); the key covers format version 4, profile, lightmap config, level JSON,
  occluder fingerprint and bake settings. A cache hit skips only the per-texel
  atlas fill: `LevelLighting::bake_with` and prop instancing always run. The
  debug dump (`PLACES_DUMP_LIGHTMAPS=1`) writes page PNGs only on a fresh fill.
- Atlas configurations: Full 16 texels/m, 1024-px pages, padding 2; Low
  9 texels/m, 512-px pages, padding 1; both at most 2 pages; geometry splits at
  a shared chart-span cap so profiles cut identically.

### 16.2 UVs and page selection

- Per-vertex `a_lightmap_uv` is the atlas UV in [0,1] inside the vertex's
  chart, quantized to u16 and normalized on the GPU.
- Per-vertex `a_lightmap_page` is the chart's page index (0/1) or
  `LIGHTMAP_NONE = 255` for vertices with no chart (props, dynamic, UI).
- The shader turns lightmaps on per fragment with
  `u_lightmap_enabled * (1 − step(254.5, page))` and selects the page with
  `mix(page0, page1, step(0.5, page))` — branchless.

### 16.3 Runtime binding and fallbacks

- `bind_lightmap_units` binds both slots (units 2/3) at the start of every
  world draw body, including the probe bake and the emissive body, and once in
  the UI pass; it restores unit 0.
- Missing pages bind the white sheet, so the samplers are always complete even
  when the shader gates them off (a driver-compatibility requirement).
- A level without a resident atlas (`lightmaps_enabled = false`) uses the baked
  light in `v_color`; the white pages are still bound.
- Upload failure deletes any partial pages, restores white slots, marks the
  atlas non-resident and (during a build) rebuilds the mesh vertex-lit.
- `set_lightmap_page` updates CPU slots only; binding waits for the next
  `bind_lightmap_units`.
- Multi-floor levels are handled by the bake: charts are allocated per
  emissive geometry (floors, ceilings, walls, skirts) and a two-page atlas
  covers the demo; a bake exceeding the budget falls back to vertex lighting
  rather than dropping pages.

---

## 17. Reflection system

Two deliberately limited sources, both opt-in per material. There are no scene
reflections, no screen-space reflections and no realtime cubemap updates.

### 17.1 Probe reflections (static, baked)

| Aspect | Detail |
|---|---|
| Resource | Up to `MAX_REFLECTION_PROBES = 2` RGBA8 cubemaps, 64 texels/face Full / 32 Low; own depth renderbuffer; CLAMP + LINEAR, no mips |
| When | Once per level load, at the end of `Renderer::set_level`, before the first frame |
| Where | Probe position = centroid of the reflective geometry cluster + 1.2 m Y; clusters split at `PROBE_CLUSTER_RADIUS_M = 12.0` |
| Capture | Six faces, each a full `draw_scene_body` (all six stages including decals) with a 90° projection, `reflection_capture = true` |
| Excluded | Reflection *sampling* is suppressed (`u_reflect_mode = 0`); no geometry is excluded |
| Consumer | The nearest probe to the camera at frame start (`bind_reflection_textures`); all probe-reflective materials share it for the frame |
| Shader | `reflect(-view, normal)`; if `roughness > 0.15`, `mix(reflected, normal, 0.5)` broadens the lookup, mixed by roughness |
| Eligibility | Material reflection mode = probe and authored strength > 0; weighted by specular × strength × fresnel/polish |
| Fallback | `black_cube` when no probe exists/was baked |
| Destroy | On the next `set_level` (`set_routing` → `clear_probes`) |
| Recursion avoidance | `reflection_uniforms` returns mode 0 for every material while capturing |
| Cross-section caveat | Probe selection is per camera, not per surface; probe-baked dynamic objects are possible (§26 Q4) |

### 17.2 Planar reflections

| Aspect | Detail |
|---|---|
| Resource | One half-resolution RGBA8 texture (`PLANAR_REFLECTION_SCALE_DIVISOR = 2`) with its own 16-bit depth renderbuffer |
| When | Every offscreen frame, before the main body; **Full profile only** (Low disables the planar pass) |
| Plane selection | `nearest_visible_reflection_plane` — frustum-tests the union AABB of each plane's reflective batches, then takes the nearest to the camera; at most one plane per frame |
| Plane derivation | From the mesh routing: planar ranges are derived and merged with `PLANE_MERGE_EPS = 1e-3`; malformed ranges are warned and skipped |
| Maths | `mirrored_mvp = mvp * mirror(normal, offset)`; mirrored camera point; frustum from the mirrored MVP; no oblique clipping |
| Capture | `front_face(CW)`, clear, full `draw_scene_body` with the mirrored MVP; all reflection sampling suppressed; batches whose material maps to the captured plane are skipped (`is_mirror_range`) so the mirror is an aperture |
| Shader | Project world position through `u_planar_matrix`, UV = `clip.xy / max(clip.w, 1e-4) * 0.5 + 0.5`; 4-tap disc when `roughness × 0.035 > 0.002`; reject outside [0,1]² and farther than ~0.08 m from the plane |
| Fallback | White sheet bound to unit 6; weight 0 when no plane is active |
| Failure | Planar target creation failure disables the planar pass for the session |
| Eligibility | Material reflection mode = planar and authored strength > 0 |

### 17.3 Reflection routing

`reflections::routing_from_mesh` inspects the mesh's reflective ranges:

- probe ranges cluster into ≤2 probe points by area;
- planar ranges derive/merge planes (normal + offset + bounds);
- `Reflections::set_profile` enables planar only on Full; the player
  reflections switch disables both probe sampling and the planar pass;
- per-material routing stores which plane a material belongs to (one plane per
  material; a material used on two planes is a documented limitation, §26 Q2);
- `u_reflect_mode` per draw comes from the material routing, the frame's active
  plane, and the capture guard.

### 17.4 Quality and player switches

| Switch | Effect |
|---|---|
| Quality Full | Planar on; probes 64 texels/face |
| Quality Low | Planar off; probes 32 texels/face |
| `reflections_enabled` | Both sources off (no probe bind beyond the black cube; no planar capture) |
| `PLACES_NO_REFLECTIONS` | Startup override for the above |

---

## 18. Decals

| Aspect | Detail |
|---|---|
| Geometry | World-space quads emitted with the static mesh (`add_decal_quad`), lifted `DECAL_SURFACE_OFFSET_M = 2.0e-4` along the surface normal; one decal cannot cross a floor/ceiling height change, and gable ceilings take no decals (level-format limitation) |
| Material | Decal material slots: slot 0 = generated 256×256 atlas (one test cell, rest transparent); slots ≥ `DECAL_EXTERNAL_BASE = 1` index external sheets; UVs for generated slots come from `decal_uv_rect`, external sheets use the full quad with both in-plane axes swapped because the PNG row order runs opposite the decal V axis |
| Program | Decal program (own fragment shader), depth test on, depth write on, **no blend**; discard below `DECAL_ALPHA_CUTOFF = 0.5` |
| Depth strategy | Geometry lift + `glPolygonOffset(-1, -4)` with `POLYGON_OFFSET_FILL`, restored after the pass. The bias is calibrated against a 24-bit fixed-point depth buffer |
| Lighting | Decal tint is computed on the CPU (surface tint × baked light sampled at a probe offset above the surface) and carried in `v_color` |
| Ordering | Decals draw last in each scene body, after translucent surfaces; they are included in planar and probe captures |
| Filtering | Decal sheets use repeat + mips + the user filter; the generated atlas is filtered with them |
| External sheets | Loaded per level from catalog/pack ids, cached by path in `decal_sheet_textures` |

---

## 19. Transparency and glass

| Aspect | Detail |
|---|---|
| Classification | Per material alpha mode: opaque (default), cutout (alpha-tested, separate program), blend (translucent) |
| Translucent batches | Collected from floors/ceilings/walls with blend + opacity > 0; sorted back-to-front per spatial batch by squared camera distance (stable order for ties); one draw per batch |
| Blend | `SRC_ALPHA`, `ONE_MINUS_SRC_ALPHA`, `FUNC_ADD`, straight alpha; **depth writes disabled**; depth test on (LEQUAL) |
| Render order | After all opaque and cutout geometry, before decals; translucent surfaces do not write depth, so they can be overdrawn by each other |
| Cutout | `discard` below the material cutoff in the Cutout program; the cutout check runs before the emission-only early-out, so cutout emitters also discard in bloom capture |
| Glass | The demo's window panes are ordinary translucent wall surfaces (material-level alpha); there is no refraction, transmission into the light bake, or per-object prop alpha |
| Emission in transparency | Translucent emissive batches are drawn into the bloom target through the same translucent pass with `u_emission_only=1`; emission-only alpha is forced to 1.0 |
| UI | UI blend is on but the UI is opaque geometry; it never participates in the scene blend |

Known limitations (documented, not fixed): translucent sorting is per batch,
not per triangle; no order-independent transparency; a translucent surface
behind another can be occluded incorrectly when batch order disagrees with
depth order.

---

## 20. Normal mapping

| Aspect | Detail |
|---|---|
| Textures | Authored per material (`response.normal`), RGBA8, repeat + mips, fitted to the Surface budget; missing maps fall back to the white sheet with `u_normal_enabled = 0` |
| Tangent frame | Static world geometry stores unit normal + unit UV-u tangent + ±1 handedness computed from winding and UV derivatives (`compute_surface_frames`); props/dynamic/UI default to normal +Z, tangent +X, handedness +1 (props: model-space frames) |
| Shader decode | `n = texture(...).xyz * 2 − 1`; `n.xy *= u_normal_strength`; Gram–Schmidt tangent against the (possibly front-face-flipped) normal; `bitangent = cross(normal, tangent) * handedness`; reconstitute `T·x + B·y + N·z`, normalize |
| No green flip | The sign is carried by the handedness attribute, not by an inverted green channel |
| Strength | `u_normal_strength` (default 1.0, cap 2.0) |
| Gate | `u_response_enabled > 0.5 && u_normal_enabled > 0.5`; the master gate is false on Low and for UI |
| Migration notes | The packed normal/tangent are normalized signed bytes; the wgpu vertex format must be `Snorm8x3` (or the exact-layout equivalent) to preserve values. `u_normal_strength` can push the sampled vector off the unit sphere before renormalization; the code normalizes after combining |

---

## 21. Debug / UI / overlay rendering

| Feature | Implementation | GL specifics |
|---|---|---|
| HUD / menus / settings | `src/ui.rs` builds vertices in 480×272 reference space; `UiGeometryCache` rebuilds only when its inputs change | Drawn with the **World program** and a plain surface state (font atlas albedo, white mask/normal), ortho MVP, depth test off, blend on, centred aspect-fit viewport with letterboxing (pixel dimensions rounded; the scale factor is fractional) |
| Performance overlay | `src/perf.rs` builds text/bar vertices, toggled with `-` | Appended to the UI vertex list; metrics from `/proc`//`sys` and frame timings, never GL queries |
| Debug geometry | none | No wireframe/gizmo/axis path |
| Console/overlays beyond UI | none | — |
| Editor rendering | separate browser WebGL1 preview (`level-editor/`) | not part of the game renderer |
| Developer screenshot | `PLACES_CAPTURE` → `capture_default_framebuffer` | `glReadPixels` of the default framebuffer, CPU row flip, PNG |
| Benchmark switches | `src/bench.rs` | skip render/swap, no-cull/no-index/exact-vertex, camera pinning, `glFinish`; no visual overlays |

UI details a port must preserve: the font atlas is `NEAREST`-filtered and
clamped; the UI is not tonemapped/bloomed (drawn after resolve); the UI
viewport is computed per frame as a centred aspect-fit 480:272 region with
rounded pixel dimensions and a fractional scale, centred in
the drawable; the UI vertex space is y-down (`orthographic_rh(0, 480, 272, 0,
-1, 1)`), and lightmap/fog/reflection/emission terms are forced off.

---

## 22. Quality profiles

Every difference between Full and Low, traced to code. There is no
auto-detection and no per-setting tiering: a profile answers how much texture
data reaches the GPU and which optional per-pixel work runs.

| Feature | Full (default) | Low | Evidence |
|---|---|---|---|
| Surface/fixture/decal sheet budget | 1024 px | 256 px | `quality.rs — budget` |
| Prop sheet budget | 256 px | 128 px | same |
| Emission mask budget | 512 px | 128 px | same |
| Scene target size | drawable | ≤480 px wide, aspect preserved, never upscaled | `framebuffer.rs — scene_target_size` (the `QualityProfile::draws_scene_at_drawable_resolution` predicate mirrors it but has no production callers) |
| Surface response (normal map + sheen) | drawn | `u_response_enabled = 0`; the white sheet is bound to the normal unit (the authored map stays resident), and the sheen term is skipped | `quality.rs — draws_surface_response`; `static_surface_state`/`apply_surface_state` |
| Planar reflections | enabled | disabled | `reflections.rs — set_profile` |
| Probe face size | 64 texels | 32 texels | `view.rs — PROBE_FACE_TEXELS_*` |
| Lightmap density | 16 texels/m | 9 texels/m | `quality.rs — lightmap_config` |
| Lightmap page edge | 1024 | 512 | same |
| Lightmap padding | 2 | 1 | same |
| Baked shadow taps/axis | 2 (quincunx) | 1 (hard) | `quality.rs — shadow_taps_per_axis` |
| Prop occlusion cell | 0.075 m | 0.15 m | `quality.rs — prop_occlusion_cell_m` |
| Resolve settings | exposure 1.0, knee 0.75, sat 1.03, contrast 1.02 | identity (plain copy unless bloom on) | `postprocess.rs — PostSettings` |
| Bloom | player switch (strength 0.42 when on) | same | `postprocess.rs — with_bloom` |
| Fog | same | same | `atmosphere.rs` |
| Emission | same | same | — |
| Geometry/level content | identical | identical | same bake patch set/chart cap |

What quality does **not** change: levels, materials, ids, fixture profiles,
fog, emission behavior, decal content, UI, blend modes, culling, or the vertex
layouts. Switching profiles at runtime calls `set_quality`, re-applies
lightmap/bloom/reflection switches, releases profile texture caches, and
rebuilds the level from the resident `LoadedLevel` (lightmaps re-baked at the
new config).

One additional profile predicate exists but has no production callers:
`QualityProfile::reduces_optional_features` (true for Low) is only exercised by
tests. It is dead policy today, documented so a port does not treat it as a
live branch.

Low's scene target cap is the one place `UI_REFERENCE_WIDTH = 480` is used
outside the UI.

---

## 23. Coordinate-system conventions

| Space | Convention | Evidence |
|---|---|---|
| World | Right-handed, +Y up, metres; yaw 0 looks toward −Z, yaw 90 toward +X, yaw 180 +Z, yaw 270 −X; positive pitch looks up | `renderer.rs — scene_view_projection`; `docs/renderer-baseline/BASELINE.md` |
| View | `look_at_rh(eye, eye + forward, +Y)` | `renderer.rs` |
| Clip / NDC | `glam::Mat4::perspective_rh_gl`: NDC z ∈ [−1, 1], OpenGL convention; frustum extraction uses `DepthRange::NegativeOneToOne`; near 0.1 m, far 100 m | `render.rs — SCENE_NEAR_M/FAR_M`; `spatial::DepthRange` |
| FOV policy | Vertical FOV configured; at aspects wider than 480:272 the vertical FOV is unchanged (Hor+); at taller aspects the horizontal FOV is preserved and the vertical grows, clamped at 150° | `view.rs — vertical_fov_for_aspect` |
| Normal / tangent | Unit, world-space for static geometry, model-space for props/dynamic; tangent along UV-u, orthogonalised; handedness ±1 selects the bitangent sign | `mesh.rs — compute_surface_frames`; `view.rs` |
| Shading normal flip | `if (!gl_FrontFacing) normal = -normal`; nothing is back-face culled | `view.rs` |
| Planar reflection UV | `clip.xy / max(clip.w, 1e-4) * 0.5 + 0.5`, i.e. GL bottom-left UV space; `u_planar_matrix` is the mirrored frame's MVP | `view.rs` |
| Probe cubemap | Standard GL cubemap face layout with explicit per-face directions/ups; baked with non-+Y up vectors so `textureCube` orientation matches | `reflections.rs — CUBE_FACES` |
| Lightmap | Page UV [0,1] inside the chart; page index per vertex; RGB8 palette | `lighting/lightmap` |
| Surface UV | World-metre tiling for surfaces: floors/ceilings `(x, z)/tile_metres`, walls `(along, up)/tile_metres`; fitted sheets use their own [0,1] UVs | `render.rs — tiled_uv` |
| Texture V | PNG decodes unflipped; row 0 uploads as v = 0 (GL convention); fixture sheets document v = 0 as the image's top row; the generated decal atlas is written bottom-up and external decal sheets swap both in-plane axes accordingly | `materials/image.rs`; `decals.rs`; `fixtures.rs` |
| NDC / framebuffer | GL bottom-left origin; the present quad grows upward to match the framebuffer texture row order | `framebuffer.rs — PRESENT_QUAD/present_matrix` |
| HUD | 480×272 reference pixels, y-down ortho, centred aspect-fit viewport with rounded pixel dimensions and a fractional scale | `view.rs — UiViewport`; `renderer.rs — render_ui` |
| Dynamic mesh | Model space, origin at floor-contact centre, +Y up, yaw about Y | `dynamic.rs`/`renderer.rs` |

Migration-sensitive differences to plan for (not solved here):

- **Clip depth:** the renderer uses OpenGL [−1, 1] NDC depth in projection and
  frustum extraction; wgpu uses [0, 1].
- **Framebuffer origin / front-face:** GL's window origin is bottom-left and
  `gl_FrontFacing` follows the `glFrontFace` winding; wgpu's render target
  origin is top-left with pipeline-level front-face/winding settings. The
  present matrix, UI ortho, planar UV convention and mirrored-winding flip all
  depend on the GL conventions.
- **Cubemap orientation:** probe faces were baked with a specific GL face
  convention; a wgpu cubemap view uses a different face/axis convention and
  sampling orientation must be verified against the Stage 0 baseline.
- **sRGB:** no sRGB in the current pipeline; typical wgpu surface formats are
  sRGB, which would change brightness and blending.
- **Texture V orientation:** uploaded PNG rows map to GL v coordinates; wgpu
  uses the same texel order, but copying/uploading with `write_texture` keeps
  row order, so only shader/UV conventions need verifying.

---

## 24. Renderer coupling classification

### 24.1 Category 1 — renderer-independent

Renderer-independent means: CPU/game data whose meaning is not tied to OpenGL;
these modules contain no `glow`/GL types.

| Module | Responsibility | Notes |
|---|---|---|
| `src/level.rs` | Level schema, validation, geometry definitions | imports lighting color/helpers for validation |
| `src/collision.rs` | Player collision resolution | pure glam math |
| `src/lighting.rs` + `light`, `color`, `math`, `tuning`, `bake`, `occlusion`, `visibility`, `lightmap/{mod,atlas,cache,fill}` | Lighting simulation, lightmap atlas bytes, cache | CPU byte buffers only; `plan.rs` is the exception below |
| `src/materials.rs` + `resolve`, `image`, `pack`, `emission`, `response`, `reflection`, `decal` | Material model, PNG decode, resolution | **no GL handles in material structs**; `TextureOrigin` encodes GPU lifetime as policy |
| `src/props.rs`, `src/gltf.rs` | Prop catalog/model cache; GLB parser | CPU `Rc<RawImage>` textures |
| `src/assets.rs` | Catalog, paths, shipped-texture contracts | |
| `src/settings.rs` | Persisted settings + `SettingsApply` record | one renderer-shaped field (`texture_filtering: String`) |
| `src/display.rs` | Window modes/size fitting policy | |
| `src/input.rs` | Key events → actions | SDL event types only |
| `src/game.rs` | Player state/movement, app state, timing | |
| `src/logging.rs` | Logging | |
| `src/ui.rs` (geometry), `src/perf.rs` (overlay geometry) | Build `Vec<Vertex>` | depend on the renderer-owned `Vertex` type (leak, §24.4) |
| `src/spatial.rs` | AABBs, cell grid, frustum, index ranges | uses `render::Vertex` (leak) and `DepthRange` (GL-depth convention) |

### 24.2 Category 2 — renderer preparation

CPU-side structures/calculations that exist to prepare rendering but do not
require OpenGL.

| Module | Responsibility | GL contact |
|---|---|---|
| `src/render/api.rs` | Level-build entry points (`build_level_geometry*`, timed/lightmap variants), bake once | none |
| `src/render/mesh.rs` | `Vertex`/`PackedVertex`, layouts, quantization, `LevelMesh`, `MeshPacker`, batch ranges | GPU-format constants (no glow) |
| `src/render/geometry.rs`, `architecture.rs`, `fixtures.rs`, `decals.rs`, `props.rs` | Emitters and batching | none |
| `src/render/dynamic.rs` | Dynamic scene CPU state | none |
| `src/render/animation.rs` | Emission animation evaluation | none |
| `src/render/atmosphere.rs` | Fog parameters | none |
| `src/lighting/lightmap/plan.rs` | Stamps chart UVs into emitted `render::Vertex` | cross-module CPU dependency (leak) |
| `src/quality.rs` | Texture budgets, bake config, profile gates | one enum mixes upload/bake/draw policy |
| `src/loader.rs` | Level loading, material resolution, validation | three validation calls into `render` (leak) |
| `src/render/view.rs` (math half) | Drawable/viewport/FOV/UI-viewport math | none |
| `src/render/framebuffer.rs` (`scene_target_size`, `present_matrix`, `PRESENT_QUAD`) | Sizing policy and quad geometry | none |
| `src/render/postprocess.rs` (`PostSettings`) | Post policy values | none |
| `src/render/reflections.rs` (routing/mirror half) | Reflection routing, plane merge, mirror matrices | none |

### 24.3 Category 3 — OpenGL implementation

Anything that directly owns or manipulates GL resource IDs, enums, programs,
textures, buffers, framebuffers, state, shader locations or texture units.

| Owner | GL surface |
|---|---|
| `src/render/renderer.rs` | The entire pipeline: programs, uniforms, attributes, buffers, textures/caches, all draw passes, reflection/planar/probe capture, emissive/bloom draws, resolve/present, readback, most state |
| `src/render/framebuffer.rs` (`SceneTarget`) | FBO, colour texture, depth renderbuffer |
| `src/render/postprocess.rs` (`ColorTarget`, `BloomTargets`, `PostProcess`) | FBOs, textures, programs, quad VBO, draw calls |
| `src/render/reflections.rs` (`ProbeTarget`, `PlanarTarget`, `Reflections`) | Cube FBO/texture/depth, planar FBO/texture/depth, face binds |
| `src/render/view.rs` (shader half) | Embedded GLSL, attribute indices, texture-unit constants, `glow::TEXTURE0 + n` |
| `src/render/renderer.rs` + `src/main.rs` | GL context creation/attributes and swap; context ownership is split between `main` and `Renderer` |
| `src/render/tests.rs` | glow enum/handle values as test data only (no context) |

### 24.4 Dependency direction and leaks

Nominal intended direction:
`level → materials → lighting → spatial → render preparation → renderer → GL`.

Observed inversions/leaks (documented, not fixed):

1. `lighting/lightmap/plan.rs` → `render::Vertex` (lighting depends on the
   renderer's vertex struct to stamp UVs).
2. `spatial.rs` ↔ `render/mesh.rs`: spatial imports `render::Vertex` while the
   mesh imports `spatial::Aabb` — a cycle around the shared vertex type.
3. `loader.rs` → `render::{AnimationEffect, MAX_*, decal_quad_points}`:
   level validation depends on renderer constants/geometry helpers.
4. `ui.rs`/`perf.rs` → `render::Vertex`: UI and telemetry build renderer-owned
   vertex types.
5. `bench.rs` → `render::RenderStats`: diagnostics depend on renderer counters.
6. `main.rs` holds GL context policy (attributes, swap interval, swap) while
   `Renderer` owns the context and re-applies attributes; there are two owners
   of context policy.
7. `Renderer::set_lightmap_page(usize, Option<glow::Texture>)` is the only
   public signature exposing a glow type; it has no external callers in-tree.
8. `main.rs` drives the camera tuple and calls `render_scene`/`render_ui`
   directly; the boundary is a method surface, not an interface.
9. `quality.rs` mixes CPU-bake budgets, GPU-upload budgets and draw-path
   switches in one enum; `settings.rs` owns the renderer's
   `texture_filtering` string format.
10. `render.rs` has a private `use glow::HasContext;` purely so `renderer.rs`
    can import it via `super` — an internal façade artifact, not a re-export.

Almost no GL type crosses out of `src/render/`: the public signatures of
`render/api.rs`, `mesh.rs`, `dynamic.rs`, `decals.rs`, `animation.rs`,
`atmosphere.rs` and the `render.rs` re-exports are glow-free. The single
exception is `Renderer::set_lightmap_page(usize, Option<glow::Texture>)`
(dead to external callers in-tree). The remaining leaks are CPU-type and
convention-level. Stage 3 implications are descriptive only: the clean existing
seam is `render/api.rs` (CPU build) → `Renderer::set_level`/GPU upload; the
load-bearing shared type to relocate is `render::Vertex`; GL ownership is
concentrated in `renderer.rs`, `framebuffer.rs`, `postprocess.rs` and
`reflections.rs`.

### 24.5 Renderer public method surface (candidate boundary)

`Renderer::{new, set_drawable_size, culling_enabled, set_quality, quality,
set_bloom_enabled, bloom_enabled, set_reflections_enabled, reflections_enabled,
release_profile_textures, set_texture_filtering, prop_draw_count, level_stats,
capture_default_framebuffer, prop_asset_stats, rebuild_level_geometry,
set_level, render_scene, set_vertex_layout, set_indexing, set_culling,
set_light_scale, set_lightmaps_requested/lightmaps_requested,
set_lightmaps_enabled/lightmaps_enabled, set_lightmaps_resident/lightmaps_resident,
set_lightmap_page, lightmap_page_count, spatial_grid, static_batch_count,
static_batch_breakdown, static_batch_family_breakdown, render_stats, finish,
render_ui, dynamic_draw_count, dynamic_object_count, dynamic_scene,
set_dynamic_demo, update_dynamic}`.

Only `set_lightmap_page` exposes a glow type; `new` exposes SDL windowing.
Everything else is neutral data or switches.

---

## 25. Migration-sensitive behavior

Consolidated list of things a port could easily get subtly wrong. This is the
high-value Stage 3/4 reference; nothing here is fixed.

### 25.1 Implicit GL state

1. Program assumed bound in `apply_surface_state`/`upload_scene_extras`.
2. Prop and decal texture binds assume active unit 0.
3. Reflection units 5/6 are established once per frame and not by scene bodies;
   the planar capture draws a scene body before its own reflection bind, and a
   planar-target recreation empties unit 6 (driver-visible "unloadable
   sampler" class; reads are gated off).
4. `run_blur` leaves depth test/blend disabled and no program bound; only the
   following resolve/present re-enables depth test.
5. `resolve` unbinds unit 1 while unit 1 is active and leaves unit 0 selected
   with the scene texture bound.
6. `render_ui` leaves the UI viewport set and attributes 3–7 enabled with UI
   VBO pointers.
7. `set_texture_filtering` unbinds unit 0 and does not invalidate the surface
   cache.
8. `set_lightmap_page` updates CPU state only; binding happens at the next
   body.
9. `set_lightmaps_enabled`/lightmap switches only invalidate the frame-state
   cache; `set_light_scale` is dead API that does not even do that.
10. Animation uses the surface-state cache epoch (`u_emission_scale`).

### 25.2 Per-pass state and blending

11. No back-face culling anywhere; two-sided shading via `gl_FrontFacing`; the
    mirrored planar pass flips `glFrontFace` instead of reversing geometry.
12. Straight alpha blend, `FUNC_ADD`, depth writes off for translucent; the
    blend result depends on values already being display-referred (no sRGB).
13. Decal depth strategy combines a geometry lift with polygon offset
    `(-1, -4)`, calibrated for a 24-bit depth buffer.
14. Decals are opaque-with-discard, so they write depth; translucent draws
    before them do not, which is why decals appear under/over correctly only
    relative to opaque surfaces.
15. Depth test `LEQUAL`, depth writes toggled per pass; clear depth 1.0.
16. The emissive pass shares the scene depth buffer and clears colour only.
17. The present/resolve/UI/blur passes run with depth test off but write depth
    mask on; the depth buffer is cleared after resolve/present.
18. Scene target is cleared twice when a planar capture ran.

### 25.3 Texture-unit semantics and fallback timing

19. Every declared sampler must always be complete; the white sheet, black
    cube and white planar fallback are load-bearing, not cosmetic.
20. Unit numbers alias across programs (unit 1 mask/bloom; unit 0 six roles);
    semantics must be per-program and phase-ordered, not copied literally.
21. The bloom sampler receives the scene texture with strength 0 when no bloom
    image exists.
22. Lightmap pages bind white when absent; the shader gates on the page byte
    and the enable flag.
23. `release_profile_textures` must be followed by a level rebuild before any
    draw; the dynamic scene is not re-synced by that path.

### 25.4 Resource lifetime and recreation

24. Scene target resize also strands the emissive FBO's attached depth
    renderbuffer until bloom targets are recreated.
25. Planar and bloom targets key off the *scene* target size, not the drawable;
    Low's capped size propagates.
26. Texture filtering changes must not re-upload pixels; a port needs
    sampler-state policy keyed to texture classes.
27. The probe bake leaves a probe FBO bound and sets `scene_mvp` to identity.
28. There are no GL deletes at shutdown; wgpu ownership must be mapped at
    creation time.
29. The lightmap cache key covers profile/config/level/occluders; warmed cache
    hits must produce identical pages (parity risk if a port re-bakes).

### 25.5 Coordinate and format conventions

30. GL clip depth [−1, 1] in projection and frustum; wgpu [0, 1].
31. Bottom-left framebuffer origin vs top-left; present matrix and UI ortho.
32. Cubemap face orientation for probes.
33. No sRGB anywhere; choosing sRGB surface/target formats changes brightness
    and blend math.
34. Normalized vertex attribute formats (`Unorm8x4`, `Snorm8x3`,
    `Snorm8`, `Unorm16x2`, `Uint8`) are part of the geometry contract.
35. Lightmap UV quantization (u16 normalized) and the page-byte sentinel
    (255 / `step(254.5)`).
36. Texture V orientation for external decal sheets (both axes swapped).

### 25.6 Level-load rendering

37. The probe bake renders before the first frame; any wgpu initialization that
    defers pipeline creation to first use must still complete during load.
38. The bake can include dynamic objects present at load time and can run
    before `set_dynamic_demo` on the menu path.
39. `set_level` uploads lightmaps before geometry buffers; an upload failure
    triggers a full vertex-lit rebuild.

### 25.7 Quality and switches

40. Low disables planar reflections and the surface response, caps the scene
    target at 480 px, and lowers lightmap density/taps/prop occlusion cells —
    all as data, not shader variants.
41. Low + bloom on still runs the resolve with identity exposure/tone/grade;
    Low + bloom off uses the plain copy.
42. Direct (`PLACES_NO_OFFSCREEN=1`) skips planar, bloom and resolve and draws
    at drawable resolution; it is intentionally not pixel-identical.
43. Filtering (`linear`/`nearest`) is a string setting that mutates sampler
    state at runtime.

---

## 26. Known quirks and technical debt

### 26.1 Intended behavior (documented in code as deliberate)

- No anti-aliasing (no multisampling requested); hard edges are the baseline
  look.
- No realtime lighting or shadow maps; everything is baked, and dynamic objects
  get one probe of the static bake with no shadows/self-occlusion.
- No sRGB/gamma handling; values are authored/calibrated in the display space.
- Low renders the scene at ≤480 px and drops the surface response by design.
- Planar reflections are one plane per frame, half resolution, Full only;
  probes are baked once per level at 64/32 texels.
- Decals cannot cross a floor/ceiling height change; gable ceilings take no
  decals.
- No per-object prop alpha, refraction or transmission into the bake.
- The world fragment stage multiplies emission by the albedo texture (material
  policy) and applies fog to emission.
- `ui.rs`/`perf.rs` reuse the world program for the HUD with a plain surface
  state.
- The direct path is a diagnostic A/B that is not pixel-identical.

### 26.2 Known or suspicious behavior (documented, not fixed)

1. `Renderer::render_scene` clears the scene target twice when a planar
   reflection ran (before and after the capture).
2. `set_texture_filtering` unbinds unit 0 without invalidating the surface
   state cache; safe only because of the current frame order.
3. `release_profile_textures` leaves `material_textures`, `fixture_sheets`,
   `level_textures`, lightmap pages, `surface_state` and `dynamic_meshes`
   referencing deleted/soon-replaced textures; `rebuild_graphics_resources`
   fills the gap with an immediate `set_level`, but the API does not encode the
   contract.
4. The live quality rebuild does not re-sync the dynamic scene, so
   `DynamicMeshGpu` submeshes can hold deleted texture names.
5. Pack decal sheets cached in `decal_sheet_textures` are not freed on level
   change (only by `release_profile_textures`); catalog decal sheets are
   session-scoped as intended.
6. `draw_decal_batches` and `draw_prop_batches` bind without selecting a unit;
   the decal program uses a literal texture unit 0 with no named constant.
7. `apply_surface_state` binds the mask/normal units with literal
   `glow::TEXTURE1`/`glow::TEXTURE4` while the sampler uniforms come from the
   named constants; the two can drift.
8. Post-process samplers are assigned per draw rather than at program creation;
   a future direct draw of those programs would use default unit 0.
9. The bloom `ensure_targets` size check compares the resident emissive
   target's size (full scene size) with the quarter-size `wanted` value, so the
   emissive and both blur targets are released and re-created on **every**
   blooming frame (statically proven; only a degenerate scene size that equals
   its own quarter could avoid it). The runtime cost is unmeasured.
10. No GL error checks; failed binds/uploads are silent.
11. No limits/capability queries; ES2 minimums are assumed.
12. `glow::GL_RGBA8`/`RGB8` sized formats and `DEPTH_COMPONENT24` are not core
    GLES2; on a strict ES2 context the 16-bit depth fallback and direct path
    are the safety net. `Needs runtime confirmation`.
13. `upload_chunks` leaks a VBO if the IBO creation fails; `create_program`
    leaks a compiled VS if the FS fails to compile (startup aborts anyway).
14. `VertexLayout` doc comments claim 24/36-byte strides; actual strides are
    36/72.
15. `bind_reflection_textures`' doc says the planar fallback is a 1-pixel black
    texture; the code binds the white sheet.
16. Procedural textures remain (`font::generate_font_atlas`,
    `decals::generate_decal_atlas`, `materials::missing_texture`), against the
    repository's PNG-texture policy; the white sheet is PNG-backed with an
    embedded fallback.
17. Reflection routing maps one plane per *material*, so a planar material
    reused on two distinct planes routes all batches to one plane.
18. Probe bakes can include dynamic objects present at load time and run before
    `set_dynamic_demo`.
19. `capture_default_framebuffer` relies on the default framebuffer being bound
    and is not gated on `skip_render`.
20. Benchmark switches `PLACES_BENCH_NOINDEX`/`EXACT_VERTEX` are applied after
    the boot level is built and therefore do not affect the boot level.
21. `Renderer::set_lightmap_page` is public but only used internally.
22. `PLACES_CAPTURE` with `PLACES_BENCH_NORENDER=1` captures a stale or
    undefined buffer.

### 26.3 Unclear behavior requiring future verification

- Whether a shipped level ever routes a planar material to two different
  planes (affects finding 17's practical impact).
- Whether the surface-state/cache staleness of the animation uniform can be
  observed in a level with no decals and no reflections (finding §10.6.15).
- Whether the bloom target churn (finding 9) is measurable and intentional.
- Exact GL formats/depth bits granted by each platform and whether strict ES2
  paths are ever taken in practice.
- Whether stale dynamic texture names after a quality change manifest as GL
  errors on target drivers.

---

## 27. Uncertainties and runtime confirmation register

These items could not be proven statically and are intentionally unresolved.
Each names the code location and the observation that would settle it.

| # | Uncertainty | Where | What would prove it |
|---|---|---|---|
| U1 | Strict GLES2 behavior for sized `RGBA8`/`RGB8` and `DEPTH_COMPONENT24`; whether the ES2 path or the desktop fallback is actually used | `main.rs — configure_gl_attributes`, `renderer.rs — Renderer::new`, `framebuffer.rs` | log `GL_VERSION`/renderer at startup and check FBO completeness on an ES2-only driver |
| U2 | Driver-visible "unloadable sampler" for reflection units 5/6 in the planar-capture window (no probe points) and after planar-target recreation | `renderer.rs — capture_reflections`, `bake_reflection_probes`, `reflections.rs — ensure_planar` | enable a GL debug/validation context, load a level with planar routing/no probes, resize mid-session |
| U3 | Bloom targets are released and re-created every blooming frame (statically proven: the resize check compares a scene-sized emissive target against a quarter-size wanted value). The cost, and whether it was ever intended, are unverified | `postprocess.rs — ensure_targets` | allocation counters or frame timing with bloom on; Git history/author intent |
| U4 | Stale dynamic texture handles after live quality change | `main.rs — rebuild_graphics_resources`, `renderer.rs — release_profile_textures/set_level` | toggle quality on the demo with the drum visible under GL debug/validation |
| U5 | Probe bake including dynamic objects from the previous scene/level | `renderer.rs — bake_reflection_probes` vs `main.rs — spawn_level_demonstration` | load a probe-bearing level after the demo, inspect probe content |
| U6 | Animation uniform staleness with a long-lived surface cache | `renderer.rs — upload_scene_extras`/`apply_surface_state` | level with an animated material and no decals/reflections; compare frames |
| U7 | Renderer behavior on macOS live drag when the drawable hits zero | `main.rs — render_and_present` | observe with `PLACES_VERBOSE` logging during a drag |
| U8 | Whether `gl_FrontFacing`/winding flip in mirrored captures shades exactly like the real surface on target hardware | `renderer.rs — capture_reflections`, `view.rs` | visual A/B of the wet-deck mirror against the baseline |
| U9 | Whether the decal fragment program's unset `u_model`/unused varyings produce driver warnings | `renderer.rs — draw_decal_batches` | GL debug output during a decal pass |
| U10 | Whether `u_emission_scale` reaching a stale cache is observable in any shipped level | `renderer.rs — upload_scene_extras` | verify animation continuity in a decal-free reflection-free level |
| U11 | Default framebuffer depth bits and whether decal polygon-offset calibration holds on 16-bit fallback platforms | `renderer.rs`/`framebuffer.rs` | query depth bits; compare decal z-fighting |
| U12 | Actual sRGB capability/format of the default framebuffer per platform | `main.rs — configure_gl_attributes` | platform query/log |

**Runtime validation performed for this audit:** the audit itself changed no
code. Verification is recorded in the final report of the Stage 2 lead agent;
`docs/VERIFICATION.md` remains the authoritative gate. No temporary
instrumentation was added and none remains.

---

## Feature parity checklist

Every item is determined from code. `N/A — not implemented in current
renderer` means a wgpu port does not need it for parity (though it may be
added deliberately later).

```text
[x] World/surface rendering
[x] Architectural geometry (stairs, ramps, half walls, columns, archways, guardrails, thresholds, baseboards; multi-storey/vertical levels)
[x] Props/models (GLB, per-model batches, CPU per-instance transform + lighting)
[x] Base textures
[x] Texture fallbacks (white sheet; diagnostic missing pattern)
[x] Normal maps (per material; white fallback; response gate)
[x] Material shine (per-material/per-surface shine -> roughness -> sheen term)
[x] Roughness compatibility (roughness is derived as 1 - shine; no PBR roughness map)
[x] Alpha behavior (opaque/cutout/blend modes, opacity multiplier, cutoff)
[x] Transparent surfaces (translucent batch pass, sorted back-to-front, depth-write off)
[x] Glass (material-level translucent surfaces; no refraction/transmission)
[x] Lighting (baked lightmap or vertex-lit fallback)
[x] Ambient/baseline lighting (part of the bake, not a shader uniform)
[x] Fixture lighting (fixture luminous faces + baked pools share one profile table)
[x] Shadows (baked only: static occlusion, prop contact occlusion, emitter penumbra taps)
[x] World shadow casting (baked)
[x] Prop shadow casting (prop occlusion volumes in the bake)
[x] Internal model shadowing (N/A — not implemented in current renderer; dynamic objects have no self-shadowing)
[x] Lightmaps (2-page RGB8 atlas; per-vertex page selector; global enable)
[x] Multi-page lightmaps (up to 2 pages; overflow falls back to vertex lighting)
[x] Lightmap fallbacks (white pages; vertex-lit mesh; upload-failure rebuild)
[x] Reflection probes (≤2 static cubemaps, 64/32 texels, baked once per level)
[x] Probe capture (6 face renders of the full scene body with reflection sampling suppressed)
[x] Reflective materials (mode + strength, specular-weighted, roughness-broadened probes)
[x] Planar reflections (Full only, one half-res plane per frame, mirrored MVP, plane rejection)
[x] Decals (generated atlas + external sheets, polygon offset, cutout discard)
[x] Emissive/special surfaces (emission masks, vertex-emission fixture faces, pulse/flicker animation)
[x] Debug geometry (N/A — not implemented in current renderer; perf overlay only)
[x] Renderer-related UI/overlays (world-program HUD at 480x272; perf overlay)
[x] Screenshot/readback behavior (PLACES_CAPTURE -> glReadPixels -> PNG; developer path)
[x] Resize handling (drawable polled per frame; scene/planar/bloom target recreation; UI viewport)
[x] Level reload (per-level texture/lightmap/probe rebuild with caches)
[x] Low quality (texture budgets, capped scene target, no response, no planar, lower lightmap/probe settings)
[x] High/Full quality (native textures, response, planar, higher lightmap/probe density)
[x] Resource cleanup/shutdown (per-level release paths; no shutdown deletes — context teardown only)
```

Additional active renderer features discovered and requiring parity:

```text
[x] Offscreen scene target + presentation copy path
[x] Direct (no-offscreen) diagnostic path with different pass coverage
[x] Emissive-only bloom capture sharing the scene depth buffer
[x] Two-pass separable bloom blur at quarter resolution
[x] Resolve tone shoulder / exposure / saturation / contrast grade
[x] Exponential-squared height-graded fog inside the world shader
[x] Per-vertex lightmap page selection with branchless page mix
[x] Runtime texture filtering switch (linear/nearest) without re-upload
[x] Dynamic-object rendering with per-object model matrix and baked-light probe
[x] Runtime quality/profile rebuild of GPU resources
[x] Runtime lightmap on/off rebuild
[x] Benchmark/telemetry paths: render stats, skip render/swap, exact/packed vertex layouts, no-cull
```

---

## Cross-reference matrix

Tracing every major feature from game/level data to visible result.

| Feature | CPU preparation | GPU resource | Shader | Pass | Quality dependence |
|---|---|---|---|---|---|
| Static world surfaces | `render/api.rs` build; `geometry.rs`/`architecture.rs` emit; `mesh.rs` pack | level VBO/IBO chunks | World / Cutout | scene body stages 1/3/5 | none (content identical) |
| Base textures | `materials::resolve_materials` decode; quality fit | material/prop/fixture texture caches | World `u_texture` | all scene stages | budgets differ (1024/256 etc.) |
| Emission + masks | `materials::emission`; `animation.rs` | mask texture, `u_emission_color` etc. | World emission term | scene body, emissive body | none |
| Fixture faces | `fixtures.rs` + lighting fixture profiles | fixture sheets, vertex emission | World `u_emission_vertex` | scene body, emissive body | sheet budget |
| Baked lighting | `LevelLighting::bake_with` | lightmap pages (RGB8) or vertex colors | World lightmap/vertex path | all world draws | density/pages/taps/cells |
| Lightmap atlas | `lightmap::plan/atlas/fill/cache` | `lightmap_pages[2]` | `u_lightmap0/1` + page attr | world draws + UI (gated off) | Full 1024/16; Low 512/9 |
| Normal maps | `materials::response`; `compute_surface_frames` tangents | normal textures | World response block | scene body (Full) | Full only (`u_response_enabled`) |
| Sheen / shine | shine → roughness | — | World sheen formula | scene body (Full) | Full only |
| Static probes | `reflections::routing_from_mesh`; bake call | `probe cubemaps` + black cube | World probe branch | planar/probe captures, world draws | face texels 64/32; player switch |
| Planar mirror | plane merge/mirror math | `PlanarTarget` + white fallback | World planar branch | planar capture, world draws | Full only |
| Decals | `add_decal_quad`, decal sheets, CPU tint | generated atlas/external sheets | Decal program | decal stage | sheet budget |
| Transparency | batch collection + CPU sort | — | World alpha + blend state | translucent stage | none |
| Dynamic objects | `dynamic.rs` scene + probes | dynamic VBO/IBO + model textures | World per-object uniforms | dynamic stage | texture budget |
| Bloom | emissive visibility flags | emissive + blur A/B | World `u_emission_only`, Blur | emissive capture, blur ×2 | player switch; resolve profile terms |
| Resolve grade | `PostSettings` | scene + bloom textures | Resolve | resolve | Full has grade/shoulder; Low identity |
| Fog | `atmosphere::FogState` | — | World fog block | all world draws | none |
| HUD / menus | `ui.rs`/`perf.rs` vertices | font atlas, UI VBO | World (plain state) | UI | none |
| Screenshot | — | default framebuffer | — | after UI, before swap | none |
| Reflection routing | `reflections::routing_from_mesh` | — | — | drives modes/binds | planar Full only |

---

## Verification record (audit day, 2026-09-24)

This audit added only this document. The repository's full verification gate
(`sh tools/verify.sh`, per `docs/VERIFICATION.md`) was run on the audit day and
exited 0, including:

- `cargo fmt --all --check` — pass;
- strict Clippy (`--workspace --all-targets --all-features`, all deny groups) —
  pass;
- `cargo test --workspace --all-features` — 839 passed, 0 failed, 3
  intentionally ignored;
- catalog, texture and prop checks — pass (35 documented soft-budget texture
  warnings);
- package tests, editor tests (144) and compiled-build tests (8, real
  SDL/OpenGL windows) — pass;
- `cargo build --release` — pass;
- `git diff --check` — pass.

`git status` compared before and after the audit shows exactly one new
untracked file, `docs/RENDERER_AUDIT.md`; no renderer, shader, asset, level or
test file was modified. The 125-file Stage 1 cleanup diff present in the
working tree is untouched.


