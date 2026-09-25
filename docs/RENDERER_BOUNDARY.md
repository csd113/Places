# Renderer boundary

Status: **Stage 3 of the renderer-modernization plan is implemented in the
working tree; Stage 4 added the first wgpu implementation behind it, Stage 5
made that implementation draw the static Places world, Stage 6 added the
texture system, Stage 7 the complete material system and Stages 8-9 the baked
lighting, lightmap atlas, reflections, props, dynamics, fixtures, emission,
decals, fog, post-processing and HUD; Stage 10 validated the result for
parity.** This document records the architecture that resulted from separating
the OpenGL implementation from the rest of Places; where its stage lists below
stop at Stage 7, treat [WGPU_STAGE9.md](WGPU_STAGE9.md) and
[WGPU_STAGE10.md](WGPU_STAGE10.md) as the current-state record. It complements,
and does not replace, the Stage 2 description of the renderer itself in
[RENDERER_AUDIT.md](RENDERER_AUDIT.md).

The existing OpenGL/GLES2 renderer is unchanged in behaviour: the same shaders,
the same passes, the same resources and the same visual output. What changed is
where each responsibility lives and which direction the dependencies point.
Stage 4 added a second implementation — a wgpu bootstrap that owns the device
and surface lifecycle — without moving the boundary or porting any content;
Stage 5 added the static world geometry pipeline, Stage 6 its textures and
Stage 7 its materials on top of that lifecycle. See
[WGPU_BOOTSTRAP.md](WGPU_BOOTSTRAP.md),
[WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md),
[WGPU_TEXTURES.md](WGPU_TEXTURES.md) and
[WGPU_MATERIALS.md](WGPU_MATERIALS.md).

## 1. Shape of the boundary

```text
level / world / gameplay / spatial / materials / lighting / camera
                         │  (engine data; no GPU types)
                         ▼
              render::common  —  renderer-neutral preparation
        geometry emitters, meshes, material draw state, reflection
        routing and mirror maths, fog, animation, camera/view maths,
        PreparedFrame (one frame's semantic render decisions)
                         │
                         ▼
              render::Renderer  —  the facade the engine talks to
        lifecycle + per-frame methods only: new / set_level / resize /
        quality + feature switches / render_scene(camera) /
        render_ui / capture / present / finish; all parameters and
        return values are engine data, never GPU objects
                         │
          ┌──────────────┴──────────────┐
          ▼                             ▼
   render::opengl                 render::wgpu
   (complete reference)           (Stage 4 lifecycle +
   programs, uniforms,            Stage 5 static world +
   buffers, passes, uploads,      Stage 6 textures +
   framebuffers, shaders,         Stage 7 materials)
   GLSL, SDL GL policy            instance, surface, adapter,
                                  device, queue, surface
                                  configuration, depth target,
                                  world buffers, camera uniform,
                                  WGSL world pipeline, texture cache,
                                  base-colour sampling, draws; no material
                                  response/lighting/shadows/lightmaps/
                                  reflections/decals/UI
          │                             │
   OpenGL / GLES2                  Metal / Vulkan / D3D12
```

`render::backend` is the temporary `PLACES_RENDERER` selector; `render::facade`
is the one place outside a backend module that names both implementations.

## 2. Module map

| Path | Role | Classification |
|---|---|---|
| `src/render.rs` | Module root and public facade surface; backend-aware window hooks; no GPU API | neutral |
| `src/render/backend.rs` | Temporary `PLACES_RENDERER` selector (`RendererBackend`) | neutral |
| `src/render/facade.rs` | `Renderer`: dispatches every engine operation to the selected backend | facade |
| `src/render/common/mod.rs` | Emitters and level-build helpers (`tiled_uv`, wall/floor emitters, decal quads, `MaterialLookup`) | neutral |
| `src/render/common/api.rs` | `build_level_geometry*` entry points and lighting/atlas build | neutral |
| `src/render/common/mesh.rs` | `Vertex`, `PackedVertex`, `LevelMesh`, `StaticBatch`, chunk packing | neutral (CPU layout) |
| `src/render/common/geometry.rs`, `architecture.rs`, `fixtures.rs`, `props.rs`, `decals.rs`, `dynamic.rs`, `animation.rs`, `atmosphere.rs` | Geometry emission, prop instancing, decals, dynamic objects, emission animation, fog | neutral |
| `src/render/common/view.rs` | `DrawableSize`, `UiViewport`, FOV and viewport maths, value-only budgets | neutral |
| `src/render/common/camera.rs` | `RenderCamera` and its view-projection/frustum | neutral |
| `src/render/common/frame.rs` | `PreparedFrame`, `FrameState`, `offscreen_plan` | neutral |
| `src/render/common/materials.rs` | `MaterialRenderState`, `BatchPass`, `ScenePass`, `EmissionRouting`, translucent collection | neutral |
| `src/render/common/reflections.rs` | Routing, planes, mirror maths, plane/probe selection | neutral |
| `src/render/common/postprocess.rs` | `PostSettings`, `bloom_target_size` | neutral |
| `src/render/common/framebuffer.rs` | `scene_target_size` | neutral |
| `src/render/common/stats.rs` | `LevelBuildStats`, `RenderStats` | neutral |
| `src/render/opengl/renderer.rs` | The GL renderer: programs, uniforms, buffers, textures, draws, captures | GL |
| `src/render/opengl/framebuffer.rs` | `SceneTarget`, present quad and matrix | GL |
| `src/render/opengl/postprocess.rs` | `ColorTarget`, `BloomTargets`, `PostProcess` | GL |
| `src/render/opengl/reflections.rs` | `ProbeTarget`, `PlanarTarget`, `ReflectionTargets` | GL |
| `src/render/opengl/shaders.rs` | GLSL sources, attribute slots, texture units | GL |
| `src/render/opengl/context.rs` | SDL GL attributes, swap interval, buffer swap | GL |
| `src/render/wgpu/renderer.rs` | `WgpuRenderer`: instance/surface/device/queue, depth target, level upload, frame graph, passes, captures, lifecycle | wgpu |
| `src/render/wgpu/surface.rs` | SDL raw-window-handle surface creation, backend policy, format/present/depth/clear policy, recovery table | wgpu |
| `src/render/wgpu/world.rs` | World geometry and passes: GPU vertex (lightmap attributes), clip correction, pack/upload, camera uniform, world pipelines, per-draw texture and material selection, translucent ordering, emissive variants | wgpu |
| `src/render/wgpu/texture.rs` | Stage 6 texture system: semantic keys, GPU uploads, CPU mip chains, shared samplers, fallback, renderer-owned cache, clamped fitted-sheet path | wgpu |
| `src/render/wgpu/material.rs` | Stage 7 material system plus Stage 9 emission fields: material uniform/layout, identity cache, normal-map and emission-mask resolution, per-draw material slots, per-frame reflection modes and animation scales | wgpu |
| `src/render/wgpu/lightmap.rs` | Stage 9 lightmap atlas: the neutral bake's RGB8 pages as raw `Rgba8Unorm` textures with the white fallback | wgpu |
| `src/render/wgpu/environment.rs` | Stage 9 group-3 environment: baked-light switch and scale, fog, atlas pages, probe cubemap(s) and planar image, per-probe bind groups and the capture fallbacks | wgpu |
| `src/render/wgpu/reflections.rs` | Stage 9 reflections: probe cubemaps (GL face convention), planar target, capture maths and the GPU round-trip orientation test | wgpu |
| `src/render/wgpu/props.rs` | Stage 9 props: neutral prop batches as GPU buffers, clamped model sheets and plain-opaque emission materials | wgpu |
| `src/render/wgpu/dynamic.rs` | Stage 9 dynamics: model-space meshes and per-object environments carrying `u_model` and the baked-light probe | wgpu |
| `src/render/wgpu/decals.rs`, `decals.wgsl` | Stage 9 decals: generated atlas and external sheets, the depth-biased pass and its cut-out fragment | wgpu |
| `src/render/wgpu/postprocess.rs`, `post.wgsl` | Stage 9 post: raw scene/presented targets, shared-depth emissive pass, two-pass blur, resolve/present copy | wgpu |
| `src/render/wgpu/ui.rs`, `ui.wgsl` | Stage 9 HUD: the 480x272 reference UI pass over the presented image | wgpu |
| `src/render/wgpu/world.wgsl` | The world shader: position -> clip, base-colour sample by UV, material colour, material normal, alpha, cut-out, lightmap atlas, sheen, reflections, emission, fog, raw/sRGB entry points | wgpu |
| `src/render/tests.rs`, `boundary_tests.rs` | In-crate tests; the boundary scans live here | test-only |

## 3. Ownership rules

The rules are enforced by `src/render/boundary_tests.rs`, which scans the
repository sources:

1. **Only `render::opengl` names the GL API.** `glow` may appear in
   `src/render/opengl/**` and in the in-crate test suite; nowhere else.
2. **Only `render::wgpu` and the facade name the wgpu API.** `wgpu::` may
   appear in `src/render/wgpu/**`, `src/render/facade.rs`, `src/render.rs` and
   the in-crate test suite; nowhere else.
3. **The engine bootstrap makes no GPU calls.** `main` asks the facade to
   request window attributes, apply a swap interval and present; it never calls
   `gl_*` or `wgpu::*` itself.
4. **Nothing outside `render` imports a backend.** Engine modules use
   `render::Renderer` and the neutral `render::*` types only.
5. **`render::common` never depends on a backend.** No GPU type and no backend
   import in the neutral layer.

### What the engine owns (meaning)

- `LevelDef`, `LoadedLevel`, `MaterialTable` and its resolved materials.
- `LevelLighting`, lightmap pages and their content keys.
- `LevelMesh`, `StaticBatch`, `PropMeshBatch`, `DynamicScene`: CPU geometry.
- `ReflectionRouting`, `ReflectionPlane`: which material reflects from where.
- `RenderCamera`, `PreparedFrame`: the frame's semantic description.
- `QualityProfile`, `PostSettings`, `FogState`, `EmissionAnimation`.

None of these contains a GPU handle; none is constructed by a backend.

### What the OpenGL backend owns (GPU realization)

- GL programs, shaders, uniform locations and their cache.
- GL textures and their caches (surface, prop, fixture, decal, lightmap,
  white/font/black-cube fallbacks).
- GL buffers (level, prop, dynamic, UI, quad) and the vertex attribute wiring.
- Framebuffers and renderbuffers (scene target, bloom targets, planar target,
  probe cubemaps).
- GL state, texture-unit bindings, readback and presentation.

### What the wgpu backend owns (GPU realization)

- The `wgpu::Instance`, `Surface`, `Adapter`, `Device` and `Queue`.
- The surface configuration and the capability-derived format, present mode
  and alpha mode.
- The main depth texture and its view.
- The acquired surface texture for the current frame and the command
  submission that clears, draws the world passes and presents.
- The world vertex/index buffers (with the lightmap attributes), the camera
  uniform and bind group, the environment binding, the world render pipelines
  for the surface, raw scene, emissive and capture formats, prop/dynamic/decal
  buffers and materials, the lightmap atlas pages, the reflection targets, the
  post targets, the generated/decalled fonts and the UI pipeline.
- The Stage 6 texture cache: GPU textures and their views, CPU-generated mip
  chains, the shared samplers, per-texture bind groups and the 2x2 fallback
  sheet. Nothing above the backend names any of them; a draw's identity is the
  renderer-neutral material key, never a GPU handle.
- The Stage 7 material cache: one uniform buffer and one pair of bind groups
  per resolved material identity, the material bind group layout, and the
  normal-map textures the Stage 6 cache owns.
- The fatal-error state derived from device loss.

These stay private to their backend modules; nothing above names them. The wgpu
backend draws the static architectural world — its opaque, material-defined
cut-out and ordinary translucent surfaces — textured and shaded by its
materials, and nothing else.

### What the engine owns in the wgpu path (unchanged)

The same neutral data OpenGL consumes: `LoadedLevel`/`LevelDef`, the resolved
`MaterialTable` (used only to classify draw passes at upload time), the
`LevelMesh` built by `render::common`, and `RenderCamera`. No wgpu type appears
in any of them.

## 4. Renderer-neutral frame preparation

`PreparedFrame` is the semantic description of one frame, produced without a
GL context:

```text
PreparedFrame
  camera                 RenderCamera (position, yaw, pitch, fov)
  drawable / render_size DrawableSize
  target_size            offscreen scene-target plan (None = direct path)
  offscreen              final decision after the backend ensures the target
  culling                whether frustum culling applies
  view_projection        glam::Mat4
  frustum                extracted with the same depth convention
  planar_plane           the mirror plane reflected this frame, if any
  probe                  the probe cubemap sampled this frame, if any
```

Preparation is deliberately two-phase because the scene target is a GPU
resource: `PreparedFrame::plan` decides everything that does not depend on the
target existing, the backend ensures the target, and
`PreparedFrame::begin_scene` finalises the render size and the matrices that
depend on it. The ordering of decisions is identical to the pre-Stage-3 frame
path, which is why the canonical captures are unchanged.

The frame does not carry draw lists or per-batch state. The backend traverses
the neutral batch lists (`StaticBatch`, `PropDraw`, decals) it already owns,
using the neutral frustum and the pre-resolved material state; a second backend
consumes the same lists. Draw classification (`BatchPass`, `ScenePass`,
`EmissionRouting`) is neutral data.

## 5. The facade

`render::Renderer` is the only rendering type the engine names. Since Stage 4 it
is an enum that dispatches to the selected implementation; its public surface
is:

- **Construction:** `new(window, video, backend)`.
- **Lifecycle:** `set_level`, `release_profile_textures`.
- **Frame inputs:** `set_drawable_size`, `render_scene(RenderCamera)`,
  `render_ui(&[Vertex])`, `finish`, `capture_default_framebuffer`, `present`.
- **Quality and feature switches:** `set_quality`, `set_lightmaps_requested`,
  `set_bloom_enabled`, `set_reflections_enabled`, `set_texture_filtering`,
  `set_culling`, `set_indexing`, `set_vertex_layout`.
- **Dynamic objects:** `set_dynamic_demo`, `update_dynamic`,
  `dynamic_scene`.
- **Diagnostics:** neutral counters and logs (`render_stats`, `level_stats`,
  batch breakdowns, `spatial_grid`, `prop_asset_stats`, `fatal_error`).
- **Windowing hooks:** `set_swap_interval` and the free
  `render::request_window_attributes` / `request_fallback_window_attributes` /
  `apply_window_flags(builder, backend)` used while the window is built.

There is no GPU-device trait, no pipeline abstraction and no
backend-independent command interface. The facade is exactly the lifecycle
Places uses. Operations the wgpu renderer cannot honour yet are answered by the
facade's wgpu arms as documented no-ops (or neutral defaults), never by
delegating to OpenGL.

## 6. Documented, intentional coupling

- The renderer-neutral vertex format (`render::Vertex`) is used by `spatial`,
  `ui`, `perf` and the lightmap chart planner so the whole pipeline speaks one
  vertex layout. It is CPU data with no GPU handle; both backends convert it
  at upload time.
- `render::Renderer::new` still takes the SDL window and video subsystem: the
  window is the surface a backend renders into, and the windowing system is
  not part of the renderer boundary.
- `main` still reads `window.drawable_size()` and polls resize each frame; a
  drawable size is a windowing fact, not a GPU object.
- The `render::*` re-export surface preserves the pre-Stage-3 paths
  (`render::Vertex`, `render::build_level_geometry`, …) so no unrelated source
  churn was necessary.
- The window's backend flags (`opengl()` vs `metal_view()`) are applied through
  `render::apply_window_flags(&mut builder, backend)` while the window is being
  built, because the platform window must exist before either backend can
  attach to it.

## 7. Stage 4 result and the later stage entry points

Stage 4 added `src/render/wgpu/` (a renderer that owns the instance, surface,
adapter, device, queue, surface configuration and main depth target), the
`Renderer` facade enum, the `PLACES_RENDERER` selector and the backend-aware
window hooks. See [WGPU_BOOTSTRAP.md](WGPU_BOOTSTRAP.md) for the lifecycle
details.

Stage 5 added the static world on top of that lifecycle: the neutral
architectural geometry is packed into 16-bit vertex/index buffers at level
load, transformed by the shared Places camera through WGSL pipelines, and
drawn with depth testing. See
[WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md) for the geometry path,
coordinate conventions and the Stage 6 handoff.

To continue:

1. Stage 6 added textures to the Stage 5 world: texture creation, uploads, mip
   levels, samplers, filtering, wrap modes, fallback textures, caching and
   sRGB/linear interpretation. See [WGPU_TEXTURES.md](WGPU_TEXTURES.md).
2. Stage 7 completed the material system: renderer-neutral resolved material
   state, the GPU material cache, normal maps through the same texture cache,
   per-surface shine overrides, alpha classification, the
   opaque/cut-out/translucent passes with the reference's ordering, fallbacks
   and reflection-eligibility metadata. See
   [WGPU_MATERIALS.md](WGPU_MATERIALS.md).
3. Nothing in level, world, material, lighting, camera, gameplay, spatial or
   asset code needed to change: none of it names a GPU type. The single
   renderer-neutral addition was material-only vertex colours and the resolved
   material state in `render::common`.
4. The `PLACES_RENDERER` default stays `opengl` until the wgpu renderer reaches
   visual parity, at which point the selector is retired.
5. Stage 8 delivered the lighting on top of the resolved material state: the
   wgpu build asks the neutral builder for the reference's historical
   vertex-lit level, and the world fragment stage assembles the reference's
   `lit + sheen` in display space. The reference has no realtime light array and
   no GPU shadow system, so the Stage 8 work is the bake's light reaching the
   GPU, not a lighting architecture. See [WGPU_LIGHTING.md](WGPU_LIGHTING.md).
6. Stage 9 completed the feature migration: the lightmap atlas (the default
   reference path), reflection probes and the planar mirror, props/GLB models,
   dynamic objects, fixture emission, decals, the emissive bloom chain and
   resolve, fog and the renderer-owned HUD. Every offscreen colour target is raw
   display space, so blending, filtering and sampling match the reference. See
   [WGPU_STAGE9.md](WGPU_STAGE9.md).
