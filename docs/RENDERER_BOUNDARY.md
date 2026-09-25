# Renderer boundary

Status: **Stage 11 of the renderer-modernization plan removed the OpenGL/GLES2
renderer from mainline; the wgpu renderer is the only rendering
implementation.** The renderer-neutral boundary that Stages 3–9 built is
unchanged: `render::common` prepares data, `render::Renderer` is the one type
the engine names, and `render::wgpu` realises it on Metal, Vulkan or Direct3D
12. The deleted OpenGL reference renderer and the complete Stage 10 dual-renderer
state are preserved at the `renderer-gles2-reference` tag; see
[RENDERER_REFERENCE.md](RENDERER_REFERENCE.md). The stage records
([WGPU_STAGE9.md](WGPU_STAGE9.md), [WGPU_STAGE10.md](WGPU_STAGE10.md)) and the
historical audit [RENDERER_AUDIT.md](RENDERER_AUDIT.md) describe the reference
renderer and are kept as history.

## 1. Shape of the boundary

```text
level / world / gameplay / spatial / materials / lighting / camera
                         │  (engine data; no GPU types)
                         ▼
              render::common  —  renderer-neutral preparation
        geometry emitters, meshes, material draw state, reflection
        routing and mirror maths, fog, animation, camera/view maths,
        target-size policy
                         │
                         ▼
              render::Renderer  —  the facade the engine talks to
        lifecycle + per-frame methods only: new / set_level / quality and
        feature switches / render_scene(camera) / render_ui / capture /
        present / finish; all parameters and return values are engine
        data, never GPU objects
                         │
                         ▼
              render::wgpu  —  the only renderer
        instance, surface, adapter, device, queue, depth target, world
        buffers and pipelines, texture cache, material cache, lightmap
        atlas, reflection probes and planar target, props/dynamics,
        decals, post-processing, HUD, capture
                         │
                         ▼
                 Metal / Vulkan / D3D12
```

`render::facade` owns the wgpu implementation and is the one place outside the
backend module that names it.

## 2. Module map

| Path | Role | Classification |
|---|---|---|
| `src/render.rs` | Module root and public facade surface; window-flag hook; no GPU API | neutral |
| `src/render/facade.rs` | `Renderer`: owns the wgpu renderer and exposes the engine's operations | facade |
| `src/render/common/mod.rs` | Emitters and level-build helpers (`tiled_uv`, wall/floor emitters, decal quads, `MaterialLookup`) | neutral |
| `src/render/common/api.rs` | `build_level_geometry*` entry points and lighting/atlas build | neutral |
| `src/render/common/mesh.rs` | `Vertex`, `LevelMesh`, `StaticBatch`, chunk packing, unit quantisation | neutral (CPU layout) |
| `src/render/common/geometry.rs`, `architecture.rs`, `fixtures.rs`, `props.rs`, `decals.rs`, `dynamic.rs`, `animation.rs`, `atmosphere.rs` | Geometry emission, prop instancing, decals, dynamic objects, emission animation, fog; the shared decal constants | neutral |
| `src/render/common/view.rs` | `DrawableSize`, `UiViewport`, FOV and viewport maths, value-only budgets | neutral |
| `src/render/common/camera.rs` | `RenderCamera` and its view-projection/frustum | neutral |
| `src/render/common/materials.rs` | `MaterialRenderState`, `BatchPass`, `EmissionRouting`, resolved surface materials | neutral |
| `src/render/common/reflections.rs` | Routing, planes, mirror maths, plane/probe selection | neutral |
| `src/render/common/postprocess.rs` | `PostSettings`, `bloom_target_size` | neutral |
| `src/render/common/framebuffer.rs` | `scene_target_size` | neutral |
| `src/render/common/stats.rs` | `LevelBuildStats`, `RenderStats` | neutral |
| `src/render/wgpu/renderer.rs` | `WgpuRenderer`: instance/surface/device/queue, depth target, level upload, frame graph, passes, captures, lifecycle | wgpu |
| `src/render/wgpu/surface.rs` | SDL raw-window-handle surface creation, backend policy, format/present/depth/clear policy, recovery table | wgpu |
| `src/render/wgpu/world.rs` | World geometry and passes: GPU vertex (lightmap attributes), clip correction, pack/upload, camera uniform, world pipelines, per-draw texture and material selection, translucent ordering, emissive variants | wgpu |
| `src/render/wgpu/texture.rs` | Texture system: semantic keys, GPU uploads, CPU mip chains, shared samplers, fallback, renderer-owned cache, clamped fitted-sheet path | wgpu |
| `src/render/wgpu/material.rs` | Material system: material uniform/layout, identity cache, normal-map and emission-mask resolution, per-draw material slots, per-frame reflection modes and animation scales | wgpu |
| `src/render/wgpu/lightmap.rs` | Lightmap atlas: the neutral bake's RGB8 pages as raw `Rgba8Unorm` textures with the white fallback | wgpu |
| `src/render/wgpu/environment.rs` | Group-3 environment: baked-light switch and scale, fog, atlas pages, probe cubemap(s) and planar image, per-probe bind groups and the capture fallbacks | wgpu |
| `src/render/wgpu/reflections.rs` | Reflections: probe cubemaps (reference face convention), planar target, capture maths and the GPU round-trip orientation test | wgpu |
| `src/render/wgpu/props.rs` | Props: neutral prop batches as GPU buffers, clamped model sheets and plain-opaque emission materials | wgpu |
| `src/render/wgpu/dynamic.rs` | Dynamics: model-space meshes and per-object environments carrying `u_model` and the baked-light probe | wgpu |
| `src/render/wgpu/decals.rs`, `decals.wgsl` | Decals: generated atlas and external sheets, the depth-biased pass and its cut-out fragment | wgpu |
| `src/render/wgpu/postprocess.rs`, `post.wgsl` | Post: raw scene/presented targets, shared-depth emissive pass, two-pass blur, resolve/present copy | wgpu |
| `src/render/wgpu/ui.rs`, `ui.wgsl` | HUD: the 480x272 reference UI pass over the presented image | wgpu |
| `src/render/wgpu/world.wgsl` | The world shader: position -> clip, base-colour sample by UV, material colour, material normal, alpha, cut-out, lightmap atlas, sheen, reflections, emission, fog, raw/sRGB entry points | wgpu |
| `src/render/tests.rs`, `boundary_tests.rs` | In-crate tests; the boundary scans live here | test-only |

## 3. Ownership rules

The rules are enforced by `src/render/boundary_tests.rs`, which scans the
repository sources:

1. **Only `render::wgpu` and the facade name the wgpu API.** `wgpu::` may
   appear in `src/render/wgpu/**`, `src/render/facade.rs`, `src/render.rs` and
   the in-crate test suite; nowhere else.
2. **The engine bootstrap makes no platform GPU calls.** `main` builds a plain
   window and asks the facade to apply the platform flags; it never calls a GL
   or Metal window API itself.
3. **Nothing outside `render` imports the backend.** Engine modules use
   `render::Renderer` and the neutral `render::*` types only.
4. **`render::common` never depends on the backend.** No GPU type and no
   backend import in the neutral layer.

### What the engine owns (meaning)

- `LevelDef`, `LoadedLevel`, `MaterialTable` and its resolved materials.
- `LevelLighting`, lightmap pages and their content keys.
- `LevelMesh`, `StaticBatch`, `PropMeshBatch`, `DynamicScene`: CPU geometry.
- `ReflectionRouting`, `ReflectionPlane`: which material reflects from where.
- `RenderCamera`: the frame's camera.
- `QualityProfile`, `PostSettings`, `FogState`, `EmissionAnimation`.

None of these contains a GPU handle; none is constructed by the backend.

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
3. draws the static world, props, dynamics, fixtures and decals through the
   neutral `BatchPass` classification — opaque, alpha cut-out and translucent
   (sorted back to front) — with the material, emission, lightmap, sheen,
   reflection and fog terms the neutral table resolved;
4. captures the emissive term, blurs it and resolves the scene into the
   presented image, exactly like the reference's post chain;
5. `render_ui` then draws the 480x272 HUD over the presented image, and
   `present` submits the acquired surface texture. `capture_default_framebuffer`
   reads back the presented raw image.

The backend consumes the same neutral batch lists (`StaticBatch`, `PropDraw`,
decals) the reference renderer consumed; no draw list crosses the facade and no
GPU type crosses in the other direction. Target-size, probe and material
decisions are made by the backend from neutral inputs, not by a shared frame
planner.

## 5. The facade

`render::Renderer` is the only rendering type the engine names. Its public
surface is:

- **Construction:** `new(window)`.
- **Lifecycle:** `set_level`, `release_profile_textures`.
- **Frame inputs:** `set_drawable_size`, `render_scene(RenderCamera)`,
  `render_ui(&[Vertex])`, `finish`, `capture_default_framebuffer`, `present`.
- **Quality and feature switches:** `set_quality`, `set_lightmaps_requested`,
  `set_bloom_enabled`, `set_reflections_enabled`, `set_texture_filtering`,
  `set_culling`.
- **Dynamic objects:** `set_dynamic_demo`, `update_dynamic`,
  `dynamic_scene`.
- **Diagnostics:** neutral counters and logs (`render_stats`, `level_stats`,
  batch breakdowns, `prop_asset_stats`, `fatal_error`).
- **Windowing hooks:** `set_swap_interval` and the free
  `render::apply_window_flags(builder)` used while the window is built.

There is no GPU-device trait, no pipeline abstraction, no backend selector and
no multi-backend command interface. The facade is exactly the lifecycle Places
uses.

## 6. Documented, intentional coupling

- The renderer-neutral vertex format (`render::Vertex`) is used by `spatial`,
  `ui`, `perf` and the lightmap chart planner so the whole pipeline speaks one
  vertex layout. It is CPU data with no GPU handle; the backend converts it at
  upload time.
- `render::Renderer::new` takes the SDL window: the window is the surface the
  renderer draws into, and the windowing system is not part of the renderer
  boundary.
- `main` reads `window.drawable_size()` and polls resize each frame; a drawable
  size is a windowing fact, not a GPU object.
- The `render::*` re-export surface preserves the pre-Stage-3 paths
  (`render::Vertex`, `render::build_level_geometry`, …) so no unrelated source
  churn was necessary.
- The window's platform flags (`metal_view()` on macOS) are applied through
  `render::apply_window_flags` while the window is being built, because the
  platform window must exist before the surface can attach to it.

## 7. Stage history

Stage 3 established the neutral/boundary split around the OpenGL renderer;
Stage 4 added the wgpu device/surface lifecycle behind a temporary
`PLACES_RENDERER` selector; Stages 5–9 ported the static world, textures,
materials, baked lighting, the lightmap atlas, reflections, props, dynamics,
fixtures, emission, decals, fog, post-processing and the HUD; Stage 10 validated
parity, repaired four ported-feature defects and recorded the bounded backend
differences. Stage 11 removed the OpenGL renderer, the selector, the GL
dependencies and the OpenGL-only diagnostics, leaving the facade above as the
engine seam. The per-stage records are the documents listed at the top of this
file; [RENDERER_REFERENCE.md](RENDERER_REFERENCE.md) maps the preserved
reference.
