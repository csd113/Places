# Architecture

Places is a Rust desktop game built from three cooperating parts: engine data,
a renderer-neutral preparation layer, and one wgpu renderer, with SDL3 as the
platform layer underneath. This document is the map: what each layer owns,
which direction dependencies point, how a frame flows, and the invariants the
layout protects.

- The renderer itself — lifecycle, passes, colour space, materials, lighting,
  reflections, quality levels and diagnostics — is documented in
  [RENDERER.md](RENDERER.md).
- Verification, the platform matrix and the capture procedure are documented
  in [VERIFICATION.md](VERIFICATION.md).

## 1. Layers

```text
level / world / gameplay / spatial / materials / lighting / camera
                         │  (engine data; no GPU types)
                         ▼
              render::common  —  renderer-neutral preparation
        geometry emitters, meshes, material draw state, reflection
        routing and mirror maths, fog, emission animation, water
        surfaces, door model builds, character posing, camera/view maths,
        target-size policy
                         │
                         ▼
              render::Renderer  —  the facade the engine talks to
        lifecycle + per-frame methods only: new / prepared installation / quality and
        feature switches / render_scene(camera) / render_ui / capture /
        present / finish; all parameters and return values are engine
        data, never GPU objects
                         │
                         ▼
              render::wgpu  —  the renderer
        instance, surface, adapter, device, queue, depth target, world
        buffers and pipelines, texture cache, material cache, lightmap
        atlas, reflection probes and planar target, props/dynamics/effects,
        decals, post-processing, HUD, capture
                         │
                         ▼
                 Metal / Vulkan / D3D12
```

Separately, and not part of the renderer:

```text
     SDL3  —  window lifecycle, events, keyboard and mouse, DPI/display,
              platform integration (main thread)
```

`render::facade` owns the wgpu implementation and is the one place outside the
backend module that names it.

## 2. Module map

| Path | Role | Layer |
|---|---|---|
| `src/render.rs` | Module root and public facade surface; no GPU API | preparation |
| `src/render/facade.rs` | `Renderer`: owns the wgpu renderer and exposes the engine's operations | facade |
| `src/render/common/mod.rs` | Emitters and level-build helpers (`tiled_uv`, wall/floor emitters, decal quads, `MaterialLookup`) | preparation |
| `src/render/common/api.rs` | `build_level_geometry*` entry points and lighting/atlas build | preparation |
| `src/render/common/mesh.rs` | `Vertex`, `LevelMesh`, chunk packing, unit quantisation | preparation (CPU layout) |
| `src/render/common/geometry.rs`, `architecture.rs`, `fixtures.rs`, `props.rs`, `character.rs`, `water.rs`, `decals.rs`, `dynamic.rs`, `doors.rs`, `effects.rs`, `animation.rs`, `atmosphere.rs` | Geometry emission, prop instancing, skinned-character posing, water surfaces, decals, dynamic objects, code-built door models, the bounded steam plume model, emission animation, fog; the shared decal constants | preparation |
| `src/render/common/view.rs` | `DrawableSize`, `UiViewport`, FOV and viewport maths, value-only budgets | preparation |
| `src/render/common/camera.rs` | `RenderCamera` and its view-projection/frustum | preparation |
| `src/render/common/materials.rs` | `MaterialRenderState`, `BatchPass`, `EmissionRouting`, resolved surface materials | preparation |
| `src/render/common/reflections.rs` | Routing, planes, mirror maths, plane/probe selection | preparation |
| `src/render/common/postprocess.rs` | `PostSettings`, `bloom_target_size` | preparation |
| `src/render/common/framebuffer.rs` | `scene_target_size` | preparation |
| `src/render/common/stats.rs` | `LevelBuildStats`, `RenderStats` | preparation |
| `src/render/wgpu/renderer.rs` | `WgpuRenderer`: instance/surface/device/queue, depth target, level upload, frame graph, passes, captures, lifecycle | backend |
| `src/render/wgpu/surface.rs` | SDL3 raw-window-handle surface creation, backend policy, format/present/depth/clear policy, recovery table | backend |
| `src/render/wgpu/world.rs` | World geometry and passes: GPU vertex (lightmap attributes), clip correction, pack/upload, camera uniform, world pipelines, per-draw texture and material selection, translucent ordering, emissive variants | backend |
| `src/render/wgpu/texture.rs` | Texture system: semantic keys, GPU uploads, CPU mip chains, shared samplers, fallback, renderer-owned cache, clamped fitted-sheet path | backend |
| `src/render/wgpu/material.rs` | Material system: material uniform/layout, identity cache, normal-map and emission-mask resolution, per-draw material slots, per-frame reflection modes and animation scales | backend |
| `src/render/wgpu/lightmap.rs` | Lightmap atlas: prepared HDR page pairs as `Rgba16Float` layers (irradiance + direction moment, alphas reserved) with the white fallback | backend |
| `src/render/wgpu/environment.rs` | Group-3 environment: baked-light switch and scale, fog, atlas pages, probe cubemap(s) and planar image, per-probe bind groups and the capture fallbacks | backend |
| `src/render/wgpu/reflections.rs` | Reflections: probe cubemaps with offline-prefiltered roughness chains (face convention), planar target, capture maths and the GPU round-trip orientation test | backend |
| `src/render/common/probe_filter.rs` | Offline cone prefilter for captured cubemaps (level roughness `L / (levels - 1)`) | preparation |
| `src/render/common/light_transport.rs` | Builds the transport scene from the prepared mesh, props and materials, and lists a level's switchable fixtures | preparation |
| `src/render/wgpu/props.rs` | Props: neutral prop batches as GPU buffers, clamped model sheets and plain-opaque emission materials | backend |
| `src/render/wgpu/dynamic.rs` | Dynamics: model-space meshes and per-object environments carrying `u_model` and the baked-light probe | backend |
| `src/render/wgpu/effects.rs`, `effects.wgsl` | Ambient effects: pre-sized steam billboard buffers, one draw per distinct effect material, straight-alpha blending with depth writes off | backend |
| `src/render/wgpu/character.rs` | Characters: shared index buffers, one mutable CPU-skinned vertex buffer and environment per character, plain-opaque submesh materials | backend |
| `src/render/wgpu/decals.rs`, `decals.wgsl` | Decals: generated atlas and external sheets, the depth-biased pass and its cut-out fragment | backend |
| `src/render/wgpu/postprocess.rs`, `post.wgsl` | Post: raw scene/presented targets, shared-depth emissive pass, two-pass blur, resolve/present copy | backend |
| `src/render/wgpu/ui.rs`, `ui.wgsl` | HUD: the 480x272 reference UI pass over the presented image | backend |
| `src/render/wgpu/world.wgsl` | The world shader: position -> clip, base-colour sample by UV, material colour, material normal, alpha, cut-out, lightmap atlas, sheen, reflections, emission, fog, raw/sRGB entry points | backend |
| `src/render/tests.rs`, `boundary_tests.rs` | In-crate tests; the boundary scans live here | test-only |

### Engine modules and content

The renderer is one consumer of a small engine. The modules it draws from, and
where the bytes live:

| Path | Purpose |
|---|---|
| `src/assets.rs` | The catalog: logical ids, classes, themes, resource paths, size policy |
| `src/level.rs` | The authored level schema (format 3): geometry rules, the walkable floor, water volumes, ladders, doors, effects, trigger volumes, timers, sequences, spawns, components/events and the typed action/condition vocabulary |
| `src/entities/` | The component-oriented entity runtime: generational identity (`id.rs`), typed component tables (`components.rs`), the bounded event queue and condition evaluation (`events.rs`), simulation-time timers (`timers.rs`), data-driven sequences (`sequences.rs`), spawn templates/points/groups (`spawn.rs`) and the world that owns doors, aiming, routes, lights, volumes, events, sequences and spawns (`mod.rs`) |
| `src/loader.rs` | Level discovery, validation (every component, binding, action, condition, sequence, timer, spawn and navigation actor record), material resolution |
| `src/nav/` | Navigation: the offline bake (`bake.rs`, compiler-only) and the runtime mesh (`query.rs`: nearest/path/segment queries over the baked grid, door portals, region statistics). `mod.rs` owns the agent profile and the live door-state trait |
| `src/ai/` | The shared AI framework: typed `AiDef` behaviors, `AiWorld` (state machine, staggered sight from real collision and hearing from gameplay stimuli, navigation-assisted flee/pursue/catch) and the locomotion/animation bridge (`movement.rs`, `perception.rs`) |
| `src/loading.rs` | One bounded preparation worker: the level's assets and its compiled variant are read concurrently, with request cancellation and an immutable decoded-variant cache |
| `src/package/` | The compiled map format: manifest, bounded ZIP access, binary records, KTX2 payloads, the player-side package loader |
| `src/compiler.rs` | The offline compiler: source validation, static preparation, probe capture, atomic package publication (`places-compile` only) |
| `src/lighting/` | The vertex-lit CPU bake (partition areas, baselines, fixture pools, box visibility) plus the prepared path: `transport.rs` (BVH, direct sampling, ray-traced bounces), `probes.rs` (the moving-object field), `lightmap/` (planning, HDR pages, content key) - compiler and audits only |
| `src/materials/` | PNG decode, session texture cache, material and decal resolution |
| `src/spatial/` | The spatial cell grid the batching and frustum culling share |
| `src/collision_index.rs` | The uniform X/Z grid over the level's solid boxes, shared by movement, support, headroom, entity routes, interaction targeting, label occlusion and route validation (allocation-free queries, exact linear fallback) |
| `src/geometry_check.rs` | The read-only map geometry checker CLI (`--check-geometry`) and its fixture suite |
| `src/zoo_audit.rs` | The test-only contracts for the generated Model Zoo and the capacity fixtures |
| `src/game/`, `src/game.rs` | Player state, movement, stance, ladders, swimming and the world tick; the entity runtime owns the map's objects, events, sequences and spawns |
| `src/door.rs` | Door runtimes: the closed/opening/open/closing phase machine, the `locked` state, current angle and collider pose, the id→index map, and per-level reset/advance |
| `src/interact.rs` | Interaction targeting (ray/reach/occlusion), the aiming table derived from `interactable` components, door aim bounds and the world-anchored label/prompt emission |
| `src/ui.rs` | The menu, level select and settings screens |
| `assets/catalog.json` | The authoritative registry mapping every logical id to a file, material or generated resource |
| `assets/environment/**`, `assets/core/**`, `assets/entities/**` | Shipped surfaces, decals, fixture faces, props and entity models (PNG/GLB) |
| `assets/levels/` | Shipped compiled map packages (`.placesmap`) and their authoring sources |
| `levels/` | Drop-in compiled packages (`.placesmap`), created on first run |
| `cache/` | Tooling cache: the Model Zoo generator stores model bounds and clip inspection results here; the player never creates it |
| `tools/` | Deterministic asset, texture, prop and level generators and validators, including `tools/levels/build_model_zoo.py` (the catalog-driven zoo generator) and `tools/levels/build_capacity_fixtures.py` |

The bake lives in `src/lighting/`; the renderer never computes light. The
renderer-neutral build in `render::common` converts the engine's `LoadedLevel`
into CPU geometry, draw state and camera data; the backend converts that into
GPU resources at upload time.

## 3. Ownership rules and the renderer-neutral boundary

The rules are enforced by `src/render/boundary_tests.rs`, which scans the
repository sources:

1. **Only `render::wgpu` and the facade name the wgpu API.** `wgpu::` may
   appear in `src/render/wgpu/**`, `src/render/facade.rs`, `src/render.rs` and
   the in-crate test suite; nowhere else.
2. **The engine bootstrap makes no platform GPU calls.** `main` builds a plain
   window and hands it to the facade; it never calls a GL or Metal window API
   itself.
3. **Nothing outside `render` imports the backend.** Engine modules use
   `render::Renderer` and the neutral `render::*` types only.
4. **`render::common` never depends on the backend.** No GPU type and no
   backend import in the neutral layer.

### What the engine owns (meaning)

- `LevelDef`, `LoadedLevel`, `MaterialTable` and its resolved materials.
- `LevelLighting`, lightmap pages and their content keys.
- `LevelMesh`, `PropMeshBatch`, `DynamicScene`, `CharacterScene`: CPU geometry.
- `ReflectionRouting`, `ReflectionPlane`: which material reflects from where.
- `RenderCamera`: the frame's camera.
- `QualityLevel` (with its two-variant `QualityProfile` content-key boundary),
  `LightmapQuality`, `ReflectionQuality`, `PostSettings`, `FogState` and its
  level-resolved `LevelFog`/`FogRegion` regions, `EmissionAnimation`.

None of these contains a GPU handle; none is constructed by the backend.

### Entity identity, components, events and sequences

Every authored object is one entity with a stable authored id. The runtime half
lives in `src/entities/`:

- **Identity is per placed instance, never per model.** `LevelDef` resolves
  each prop's `id` (authored, else deterministic `<model-short>_<n>`), and
  doors, fixtures, volumes, timers, spawn points and effects resolve their own
  ids into the same per-level namespace (a duplicate across any of them is a
  load error). `EntityWorld` binds every id to a generation-checked
  `EntityHandle`; removing an entity or replacing the world invalidates the
  old handle, so a queued event or a running sequence from an unloaded world
  can never touch a coincidentally equal slot in a new one. All run state is
  keyed by that id or handle — never by model, catalog id, filename or array
  position.
- **Components are typed capabilities, not a script bag.** A prop, door,
  fixture, volume, timer, spawn point or spawned instance carries
  `interactable`, `animation`, `audio`, `light`, `material`, `state`,
  `lifetime`, `steam`, `water`, `nav_agent`, `nav_obstacle` or `ai` components
  plus its `transform`, `renderable` and `collider` where they apply. Storage is one
  sparse table per component kind (`src/entities/components.rs`), iterated by
  kind and never scanned per frame in full; a static prop's geometry, collider
  and bake stay compiled into the prepared world, and only runtime entities
  (doors, floating props, spawned instances, routed characters) own live
  transforms.
- **All event sources feed one pipeline.** `interact` (the Interact key on the
  aimed entity), `enter_volume` / `exit_volume` (trigger-volume edges, swept so
  a fast fall cannot miss a thin band), `timer`, `object_state` (only on a real
  change), `sequence_complete`, `spawn`, `animation_complete`, and the
  AI-state/catch events reserved for the navigation upgrade are queue records
  carrying the world generation. The dispatcher pops them FIFO in bounded
  waves, evaluates each binding's conditions, and runs its actions in authored
  order. A full queue or an exhausted budget refuses work and reports once; a
  self-referential chain is cut at `MAX_CHAIN_DEPTH` instead of recursing.
- **Actions are a closed typed set.** `open`, `close`, `toggle`, `enable`,
  `disable`, `set_light`, `lock`, `unlock`, `play_animation`,
  `toggle_animation`, `play_sound`, `stop_sound`, `change_material`,
  `move_object`, `set_state`, `toggle_label`, `start_sequence`,
  `stop_sequence`, `start_timer`, `stop_timer`, `spawn_entity`,
  `despawn_entity` and `reset_to_start`. An omitted target is the acting
  entity; an explicit target must resolve on its own. A missing target, an
  action the target cannot perform, and an unsupported combination are named
  validation errors for shipped maps and counted, reported outcomes at runtime
  — never silent no-ops.
- **Lights.** `set_light`/`toggle` write the `Light` component and mark the
  fixture dirty; the frame loop pushes dirty `(fixture index, enabled)` pairs
  to `Renderer::apply_light_toggles`, which selects the prepared switchable
  lightmap layers. Illumination follows the promised state for a switchable
  fixture; every other light is static, and validation rejects a switchable
  light anywhere but a fixture. There is no per-switch shader branch and no
  runtime rebake.
- **Sequences** (`src/entities/sequences.rs`) are authored resources: ordered
  `action`, `wait`, `move`, `face`, `wait_animation`, `emit`, `set_state` and
  `stop` steps that share the ordinary action pipeline. A sequence runs on one
  entity (the controller); starting a second replaces the first, despawning
  the owner or the map cancels it, `wait_animation` is bounded by its timeout,
  and completion emits `sequence_complete` exactly once. Movement respects the
  same collision world as the player and the authored entity routes.
- **Spawns** (`src/entities/spawn.rs`) are typed prefabs: a template names a
  model, a scale, an optional lifetime and the components and bindings the
  instance is born with; a point says where it appears; a group with
  `at_most_one_active` is the reusable encounter rule. Adding a member while
  one lives is refused with a diagnostic, despawn (lifetime, action or reset)
  releases the group, and a spawn becomes visible on the frame it is requested
  because the frame loop drains the spawn commands before it draws.
- **Doors** (`src/door.rs`) keep the phase machine, the obstruction policy and
  the collider derived from the live angle; a door's `locked` state is on the
  runtime, `open`/`close`/`toggle`/`lock`/`unlock` drive it, and
  `EntityWorld::door_blockers()` publishes the id, passability and collider.
  `src/render/common/doors.rs` builds the visuals as two models: the static
  frame (a reveal liner through the wall tunnel the leaf is installed in, its
  stop lip, a casing on each end face and the hinge furniture) and the moving
  leaf (a panelled or sauna glass build), each submesh sampling its own material
  slot so the leaf, frame, handle and glass textures stay independent.
  Navigation reads the same state through door **portals** in the baked mesh, so
  a closed door blocks a route and opening it invalidates cached paths without
  ever rebaking the mesh.
- **Navigation and AI** (`src/nav/`, `src/ai/`) are one shared system. The
  compiler bakes one cell mesh per map: a class per distinct `nav_agent` body
  (clearance, headroom, step and slope evaluated at bake time), connected region
  labels, and a portal per door leaf. `EntityWorld` owns the `AiWorld`; on each
  tick it perceives (staggered sight against real collision and live leaves,
  hearing against the bounded stimulus queue), decides from authored tags
  (`role`/`reacts_to`), moves through the same collision and floor rules the
  player uses, and emits transform updates, `ai_state`/`caught` events, door
  requests and its own movement noise. A catch freezes the prey once and leaves
  the pounce/consume presentation to the ordinary sequence system. The player
  only decodes and queries the baked mesh.
- **Routes and animation** are unchanged in behaviour: `entity::PoseCue` is
  still the one pose vocabulary, `play_animation`/`toggle_animation` set a
  per-instance override that wins over the route's own cue, and a one-shot cue
  that completes is fed back from the renderer as `animation_complete`.
- **Reset** (`reset_to_start`) re-seeds every runtime: authored spawn and yaw,
  zeroed velocity, cleared water/ladder/stance state, every door to its
  authored state, every route to its authored start, every timer, every spawn
  group and every binding's `once`/cooldown state, every running sequence
  stopped, every light restored to its authored state, every state bag
  restored to its authored values, and every volume baseline from the new
  position.
- **Presentation** is `interact::append_world_labels(vertices, game, camera,
  drawable)`: world anchors projected into the 480x272 reference space and
  drawn with the existing `ui::draw_text`/`render_ui` pipeline. Labels respect
  occlusion through `collision::ray_aabb_entry`; there is no second text
  renderer.

`src/settings.rs` owns the persisted player configuration and the runtime
settings model. Overall Quality (Low / Medium / High) is the preset for the
three Advanced settings (Texture Filtering, Lightmaps, Reflections): an active
quality change cascades them, each may then be overridden independently, and
Bloom and VSync are ordinary independent preferences. `main` reads the
effective values (session-only `PLACES_*` startup overrides folded in) and
hands them to the renderer; see [RENDERER.md](RENDERER.md) §12 for the mapping
and §13 for the startup switches.

### What the wgpu backend owns (GPU realization)

- The `wgpu::Instance`, `Surface`, `Adapter`, `Device` and `Queue`.
- The surface configuration and the capability-derived format, present mode
  and alpha mode.
- The main depth texture and its view.
- The acquired surface texture for the current frame and the command
  submission that clears, draws the world passes, resolves/blooms and presents.
- The world vertex/index buffers (with the lightmap attributes), the camera
  uniform and bind group, the environment binding, the world render pipelines
  for the surface, raw scene, emissive and capture formats, prop/dynamic/decal
  buffers and materials, the lightmap atlas pages, the reflection targets, the
  post targets, the generated/decalled fonts and the UI pipeline.
- The texture cache: GPU textures and their views, CPU-generated mip chains,
  the shared samplers, per-texture bind groups and the fallback sheet. Nothing
  above the backend names any of them; a draw's identity is the
  renderer-neutral material key, never a GPU handle.
- The material cache: one uniform buffer and one pair of bind groups per
  resolved material identity, the material bind group layout, and the
  normal-map textures the texture cache owns.
- The fatal-error state derived from device loss.

These stay private to the backend module; nothing above names them.

## 4. Frame flow

`Renderer::render_scene(camera)` is one call. Inside it the backend:

1. takes the drawable from the surface and derives the scene target from the
   neutral `scene_target_size` policy (the drawable at Full, the ≤480 px budget
   at Low);
2. reflects the active plane and bakes/samples the probes selected from the
   neutral routing (`nearest_visible_reflection_plane`, per-material reflection
   modes), reusing the targets only while their size and profile hold;
3. draws the static world, props, dynamics (including the level's door frames
   and leaves), characters, fixtures and decals
   through the neutral `BatchPass` classification — opaque, alpha cut-out and
   translucent (sorted back to front) — with the material, emission, lightmap,
   sheen, reflection and fog terms the neutral table resolved;
4. captures the emissive term, blurs it and resolves the scene into the
   presented image; the presented target is the drawable in both profiles;
5. `render_ui` then draws the 480x272 HUD over the presented image, and
   `present` submits the acquired surface texture. `capture_default_framebuffer`
   reads back the presented raw image.

The backend consumes the same neutral batch lists (`PropMeshBatch`, `PropDraw`,
decals) the preparation layer produced; no draw list crosses the facade and no
GPU type crosses in the other direction. Target-size, probe and material
decisions are made by the backend from neutral inputs, not by a shared frame
planner.

The pass graph, targets and colour-space rules are detailed in
[RENDERER.md](RENDERER.md) §2-§3.

## 5. The `Renderer` facade

`render::Renderer` is the only rendering type the engine names. Its public
surface is:

- **Construction:** `new(window)`.
- **Lifecycle:** `install_prepared`, `advance_prepared_install`,
  `cancel_prepared_install` stage and commit worker-prepared worlds.
  `release_profile_textures` retires quality-fitted resources.
- **Frame inputs:** `set_drawable_size`, `render_scene(RenderCamera)`,
  `render_ui(&[Vertex])`, `finish`, `capture_default_framebuffer`, `present`.
- **Quality and feature switches (recording only):** `set_quality`,
  `set_lightmap_quality`, `set_reflection_quality`, `set_bloom_enabled`,
  `set_texture_filtering`, `set_culling`.
- **Graphics transaction:** `apply_frame_graphics` applies changes that do not
  require CPU preparation. Level and lightmap replacements use the loading
  worker and staged GPU installation (see RENDERER.md §12.2).
- **Dynamic objects and effects:** `set_dynamic_demo`, `update_dynamic`,
  `dynamic_scene`, `sync_doors`, `set_level_effects`.
- **Animated characters:** `update_characters(delta_seconds,
  LocomotionSnapshot)`, `character_count`.
- **Diagnostics:** neutral counters and logs (`render_stats`, `level_stats`,
  batch breakdowns, `prop_asset_stats`, `fatal_error`).
- **Windowing hooks:** `set_swap_interval`.

There is no GPU-device trait, no pipeline abstraction, no backend selector and
no multi-backend command interface. The facade is exactly the lifecycle Places
uses.

### Loading ownership

The event loop creates the window and renderer before requesting the initial
world. `loading::Loader` owns one serialized worker that decodes compiled
packages: it opens the package, resolves the level's texture pixels through the
installed asset bundle, decodes the requested quality variant's records
(geometry, props, baked lighting, lightmap atlas, collision, reflection probe
captures), builds collision and character playback state, and hands the frame
loop a world to install. It never emits geometry, bakes light, plans charts,
fills an atlas, captures a probe or derives static collision: that is the
offline compiler's work (`src/compiler.rs`, the `places-compile` binary).

Requests carry generations; superseded work is cancelled cooperatively and
cannot commit. The event loop polls completion without joining the worker and
continues presenting the loading UI or previous world.

Decoded geometry is shared through `Arc<LevelBuild>`. A worker-owned LRU is
bounded by entry count and retained-data bytes, keyed by the package's content
identity and the lightmap quality; probe captures and compiled collision travel
with the decoded variant because they belong to the same prepared world. GPU
work stays on the main thread: installation advances through upload phases, then
commits the completed world. Packaged probe cubemaps are uploaded, not rendered.
Cancellation discards pending installation and preserves the active world. A
phase can still contain an indivisible expensive GPU operation; staging is not a
hard per-frame latency guarantee.

### Documented, intentional coupling

- The renderer-neutral vertex format (`render::Vertex`) is used by `spatial`,
  `ui`, `perf` and the lightmap chart planner so the whole pipeline speaks one
  vertex layout. It is CPU data with no GPU handle; the backend converts it at
  upload time.
- `render::Renderer::new` takes the SDL3 window: the window is the surface the
  renderer draws into, and the windowing system is not part of the renderer
  boundary.
- `main` reads `window.size_in_pixels()` and polls resize each frame; a
  drawable pixel size is a windowing fact, not a GPU object.
- The `render::*` re-export surface preserves the long-standing module paths
  (`render::Vertex`, `render::build_level_geometry`, …) so engine code does
  not depend on how the renderer is organised internally.

## 6. The platform layer: SDL3

SDL3 owns the window, the event pump, keyboard and mouse input, DPI/display
queries and platform integration. It is not part of the renderer: the renderer
receives a window and uses the raw-window-handle path to create its surface.
There is no GL context, no `SDL_GPU`, no `SDL_Renderer` and no SDL canvas
rendering anywhere in the project; frame pacing is `std::time::Instant`, and
Places keeps its ordinary Rust `main()` and frame loop.

### Dependency and linking

```toml
sdl3 = { version = "0.20", features = ["use-pkg-config", "raw-window-handle"] }
```

- `sdl3` 0.20.0 (MIT) with `sdl3-sys` 0.7.1+SDL-3.4.16 (bindings for SDL
  3.4.16, zlib).
- `use-pkg-config` resolves the system SDL3 (`brew install sdl3`); `sdl3-sys`
  probes pkg-config by default, and the feature is named explicitly so the
  intent is visible.
- `raw-window-handle` pulls raw-window-handle 0.6.2 — the version wgpu 30.0.1
  uses — and implements `HasWindowHandle`/`HasDisplayHandle` for
  `sdl3::video::Window`.
- The bindings' own floor is SDL 3.1.3, but Places calls `SDL_SyncWindow`
  (SDL 3.2.0), so the practical minimum is **SDL 3.2.0**.
- No `image`, `ttf`, `mixer`, `net`, `gfx`, `main`, `ash`, `static-link`,
  `build-from-source*` or `link-framework` feature is enabled.

The release binary links the system SDL3 dynamic library (on macOS,
`/opt/homebrew/opt/sdl3/lib/libSDL3.0.dylib`). This matches the existing
deployment model — executable + assets, with SDL as a system dependency.

### Initialization, metadata and identity

`main` calls

```rust
sdl3::set_app_metadata(
    Some("Places"),
    Some(env!("CARGO_PKG_VERSION")),
    Some("io.github.csd113.places"),
)?;
```

before `sdl3::init()`. The identifier `io.github.csd113.places` is the desktop
bundle and X11 app id. Environment switches use the `PLACES_*` prefix. Telemetry is generic
and portable: there are no device-specific GPU probes; the remaining readings
are Linux devfreq/DRM and macOS CPU counters.

Initialization order is: app metadata -> SDL3 init + video subsystem -> create
the window (`position_centered`, `resizable`, `high_pixel_density`, optional
borderless fullscreen) -> optionally `SDL_SyncWindow` for a boot-time
fullscreen request -> renderer construction from the window -> event pump ->
frame loop.

Text input stays disabled: SDL3 leaves it off until `SDL_StartTextInput`, and
Places stops it explicitly after window creation as defence in depth, so the
macOS IME path stays disengaged and key events remain raw.

### Window and size semantics

| Concept | API |
|---|---|
| logical size | `window.size()` (window coordinates) |
| physical drawable | `window.size_in_pixels()` |
| HiDPI request | `high_pixel_density()` |
| borderless fullscreen | `builder.fullscreen()` (no display mode set = borderless desktop) |
| runtime fullscreen | `set_fullscreen(true/false)` |
| display lookup | `window.get_display()`, fallback `video.get_primary_display()` |
| display bounds | `Display::get_bounds()` / `get_usable_bounds()` |
| window position | `position_centered()` |

The renderer reads only the physical pixel size. On the development host the
canonical 640x360 logical window is 1280x720 pixels at a 2.0x backing scale,
and the default 1920x1080 request is clamped to the display work area and
reported by the OS as 1512x838 logical / 3024x1676 pixels.

SDL3 window operations are asynchronous requests. Places observes window state
by polling once per frame, and the one place a barrier is used is boot-time
fullscreen: `create_window` calls `SDL_SyncWindow` (`window.sync()`) once when
the saved mode is fullscreen, so the first frame and a first-frame capture see
the final display-size drawable. A timed-out sync is logged and non-fatal; the
per-frame poll adopts the state when it lands.

### Events and input

The event surface is `Event::Quit`, `Event::KeyDown` and `Event::KeyUp` only.
Input is **keycode-based** (logical, layout-dependent), never scancode-based,
so `settings.json` bindings stay stable across keyboards. Key repeat semantics:
the first `KeyDown` (`repeat: false`) presses or triggers, repeats do nothing,
and `KeyUp` releases. `Keycode::name()` supplies fallback names, and the
special spellings (`ESC`, `-`, `KP_MINUS`, `ENTER`, …) are unchanged.

### macOS Metal layer

SDL3's raw-window-handle implementation reports the window's content view
(`RawWindowHandle::AppKit`) regardless of any metal-view helper; wgpu's Metal
backend makes it layer-backed and attaches its own `CAMetalLayer` through
`raw-window-metal`, which tracks the view's bounds and backing scale. No
platform window flag, property or pre-build hook is required, so `main` builds
a plain `video.window(...)` and no renderer-specific window setup exists.

### Lifetime rules and the unsafe surface handoff

- SDL3 initialization, window operations, the event pump and the game loop run
  on the main thread; Places spawns no platform or rendering threads.
- The window is declared before the renderer in `main`, so Rust drops the
  renderer first on every path, including error returns; the renderer never
  touches the window in `Drop`.
- SDL's video subsystem and the window live for the whole frame loop, which is
  the only time the surface is used.
- The two `unsafe` operations needed by the raw-window-handle surface path
  (handle extraction and `create_surface_unsafe`) are confined to one
  `unsafe fn` in `src/render/wgpu/surface.rs`, with a written `SAFETY`
  argument. There are no transmutes, no faked `'static` in the type system (the
  surface type is `Surface<'static>` by the raw-handle API's contract, upheld
  by the drop order above), no leaked window and no reliance on undocumented
  destruction order. The surface is recreated from the same window on loss.

## 7. Build targets, dependencies and portability

Places builds for the three desktop targets and selects one native wgpu
backend per target at compile time:

| Target | Window/input | Renderer |
|---|---|---|
| macOS | SDL3 | wgpu -> Metal |
| Linux | SDL3 | wgpu -> Vulkan |
| Windows | SDL3 | wgpu -> D3D12 |

- Rust 1.91 or newer (edition 2024); the project is verified on 1.98.x.
- `wgpu` 30.0.1 with `default-features = false` and `["std", "wgsl"]`, plus
  exactly one native backend feature per target (`metal`, `vulkan`, `dx12`).
  The backend set is exactly the one native backend selected at build time; no
  alternate-API backend is compiled in, so a silent fallback to another API is
  impossible by construction.
- `pollster` 1.0 blocks on the two wgpu initialization futures without an
  async runtime.
- SDL3 provides the platform layer on every target, so the game is portable by
  construction: platform-specific code is confined to the windowing crate and
  the backend feature selection. Per-platform build instructions and the
  current verification status are in [VERIFICATION.md](VERIFICATION.md).

## 8. Historical note

The former GLES2 renderer is preserved in Git at the
`renderer-gles2-reference` tag (commit
`797370e17aab1409a5de3ea70b9a68f742452`). Mainline does not depend on it; see
[RENDERER_REFERENCE.md](RENDERER_REFERENCE.md) for the map to the snapshot.

Runtime atlas filling uses at most three scoped chart producers, capped below
available CPU count to leave room for the event loop. Each has one queued result
and one active chart; the preparation worker packs results in original chart order.
Cancellation is checked per texel row. Receivers are dropped before joins on errors
and cancellation so blocked senders can exit. Inline/headless bakes and ordinary
unit-test invocations remain serial; explicit parity tests exercise the bounded
parallel path. This avoids multiplying chart pools across concurrently running tests.
