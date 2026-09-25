# wgpu world geometry (Stage 5)

Status: **Stage 5 is implemented in the working tree; Stage 6 added base-colour
textures, Stage 7 the complete material system, Stage 8 the baked lighting
and sheen, Stage 9 the lightmap atlas, reflections, props, dynamics, fixtures,
emission, decals, fog, post-processing and HUD, and Stage 10 validated the
result for parity** (see [WGPU_TEXTURES.md](WGPU_TEXTURES.md),
[WGPU_MATERIALS.md](WGPU_MATERIALS.md), [WGPU_LIGHTING.md](WGPU_LIGHTING.md)
and [WGPU_STAGE9.md](WGPU_STAGE9.md)/[WGPU_STAGE10.md](WGPU_STAGE10.md)).
The vertex-layout and scope lists further down are the Stage 5 record; the
Stage 9/10 documents supersede them. The wgpu backend draws the static Places
world — floors, ceilings, walls and the architectural pieces built from the
level (ramps, stairs, half walls, columns, archways, guardrails, thresholds,
baseboards), plus the material-defined cut-out and translucent ranges (window
panes, the vent grille) — through the shared Places camera, with depth testing,
per-material pipeline states and the reference's baked lighting. Lightmap
atlases, reflections, decals, props, fixtures and UI are not ported yet.

The OpenGL/GLES2 renderer remains the complete reference implementation and the
default (`PLACES_RENDERER` unset). Stage 4's lifecycle
([WGPU_BOOTSTRAP.md](WGPU_BOOTSTRAP.md)) was reused unchanged: instance, surface,
adapter, device, queue, presentation, resize, minimize/restore, surface-loss and
device-loss handling needed no rework for Stage 5.

This document is the Stage 5 handoff. It is deliberately explicit about
coordinate conventions, because they are the part a later stage must not
rediscover.

## 1. Stage 5 scope

Implemented:

- `wgsl` enabled for wgpu (Stage 4 compiled no shader module);
- one world shader module and one world render pipeline;
- an explicit world vertex layout and vertex buffer;
- a 16-bit index buffer;
- static world mesh upload at level load, with level-change replacement;
- the camera uniform and its binding;
- projection, depth testing, depth writes, face winding, face culling and
  indexed draws of the architectural world;
- resize-compatible camera/projection behaviour (the projection follows the
  physical drawable every frame);
- the one deliberate OpenGL -> wgpu clip-space conversion.

Deliberately absent (later stages): the material response (shine, roughness,
normal maps, alpha), props, lighting, shadows, lightmaps, reflections, decals,
transparency, glass, emissive rendering, UI, renderer debug overlays, and
`PLACES_CAPTURE` for wgpu. Stage 6 added base-colour textures only; see
[WGPU_TEXTURES.md](WGPU_TEXTURES.md).

## 2. World geometry data flow

```text
LevelDef (src/level.rs)
        │  LoadedLevel { level, materials, … }              (src/loader.rs)
        ▼
Renderer::set_level(&LoadedLevel)                          (src/render/facade.rs)
        ▼
WgpuRenderer::set_level                                    (src/render/wgpu/renderer.rs)
        │  build_level_geometry_timed_with_lightmaps(level, catalog, assets,
        │      materials, LightmapBuildOptions::for_profile(quality, Off), None)
        ▼
LevelMesh { ranges: Vec<LevelMeshRange>, … }               (src/render/common/mesh.rs)
        │  is_stage5_world_range + MeshPacker               (src/render/wgpu/world.rs)
        ▼
pack_world_ranges(mesh, materials) -> (MeshPacker, Vec<WorldDraw>)
        │  each WorldDraw keeps its range's material key (Stage 6)
        ▼
WgpuWorldGeometry::upload(device, queue, mesh, materials)
        │  one GPU vertex/index buffer pair per 16-bit chunk
        ▼
WorldTextures::resolve(cache, device, queue, draws, materials, table, quality)
        │  per draw: base texture or the shared fallback     (Stage 6)
        ▼
WorldPipeline::encode(pass, geometry, textures, filtering, frustum, cull)
```

The renderer-neutral mesh is the same type the OpenGL reference renderer
builds, through the same `render::common` entry point. Stage 5 asks that entry
point for the **historical vertex-lit build** (`LightmapMode::Off`): lightmaps
are a later stage, and baking an atlas that nothing samples would be wasted
work. The build's props are resolved for parity with OpenGL's counters but are
not uploaded.

## 3. CPU vertex representation

The renderer-neutral `Vertex` (72 bytes) stays untouched:

| field | type | Stage 5 use | Stage 8 use |
|---|---|---|---|
| `pos` | `[f32; 3]` | uploaded | uploaded |
| `color` | `[f32; 4]` | not uploaded (baked shade; Stage 7+) | uploaded (material factor × baked light, `Unorm8x4`) |
| `uv` | `[f32; 2]` | uploaded (Stage 6 texturing) | uploaded |
| `normal` | `[f32; 3]` | uploaded (temporary tone; Stage 7 lighting) | uploaded (material normal) |
| `tangent` | `[f32; 3]` | not uploaded (normal mapping is Stage 7+) | uploaded |
| `handedness` | `f32` | not uploaded | uploaded |
| `lightmap` | `[u16; 2]` | not uploaded (lightmaps are Stage 9) | not uploaded |
| `lightmap_page` | `u8` | not uploaded | not uploaded |

Stage 7 uploaded the material factor (`tint × directional face shade`); Stage 8
switched the build to
`LightmapBuildOptions::for_profile(quality, LightmapMode::Off)`, so the colour
is the reference's historical vertex-lit
`clamp(tint × face shade × baked light)` and carries the room baselines,
fixture pools, opening blends and static-occluder shadows. Its 8-bit
quantisation is the reference's own. The UV-space tiling is unchanged; see
[WGPU_LIGHTING.md](WGPU_LIGHTING.md) §3.

Nothing was removed from the neutral vertex; Stage 5 simply carries the fields
it has a use for.

## 4. GPU vertex representation

`render::wgpu::world::WorldVertex`, `#[repr(C)]` + `bytemuck::Pod`, 52 bytes:

| attribute | location | format | offset |
|---|---|---|---|
| position | 0 | `Float32x3` | 0 |
| normal | 1 | `Float32x3` | 12 |
| uv | 2 | `Float32x2` | 24 |
| color | 3 | `Unorm8x4` | 32 |
| tangent | 4 | `Float32x3` | 36 |
| handedness | 5 | `Float32` | 48 |

One interleaved vertex buffer, stride 52, step mode `Vertex`. The layout is
`world::world_vertex_layout()`; `WORLD_VERTEX_STRIDE`,
`WORLD_ATTRIB_POSITION`, `WORLD_ATTRIB_NORMAL`, `WORLD_ATTRIB_UV`,
`WORLD_ATTRIB_COLOR`, `WORLD_ATTRIB_TANGENT` and `WORLD_ATTRIB_HANDEDNESS` name
the contract, and unit tests pin `size_of`, every `offset_of!` and every
attribute against it. Stage 7 added the colour (the material factor, quantised
to a byte exactly like the reference packed layout) and the tangent frame; there
is still no lightmap coordinate and no material/light index on the GPU (a
material is a draw-level binding).

## 5. Index format

`IndexFormat::Uint16`, exactly like the OpenGL reference (`GL_UNSIGNED_SHORT`,
the only width core GLES2 guarantees). The neutral `MeshPacker` chunks the draw
set so an index never exceeds 65 536 vertices; a range larger than one chunk is
split and each placement becomes its own draw, so no base-vertex offset is ever
needed. There is no conversion to `Uint32` and therefore no overflow case.

## 6. Buffer ownership

`WgpuWorldGeometry` owns `Vec<WorldChunk>` (one `vertex_buffer`, one
`index_buffer`, counts) and `Vec<WorldDraw>` (`chunk`, `index_start`,
`index_count`, `vertex_count`, `bounds`, `kind`, and since Stage 6 the range's
`material` key; since Stage 7 its `shine` override and its `pass`). Usage:
`VERTEX | COPY_DST` and `INDEX | COPY_DST`. Buffers are persistent across
frames and rebuilt only by `set_level`; replacing the `WgpuWorldGeometry`
releases the previous level's buffers. An empty draw set yields no buffers and
no draws, which clears/presents safely.

## 7. Camera uniform representation

```rust
#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_projection: [[f32; 4]; 4],  // offset 0,  64 bytes
    pub position: [f32; 3],              // offset 64, 12 bytes
    pub _padding: f32,                   // offset 76
}                                        // 80 bytes total
```

Column-major, matching WGSL `struct Camera { view_projection: mat4x4<f32>,
position: vec3<f32>, _padding: f32 }` with
`@group(0) @binding(0) var<uniform> camera: Camera`. The buffer is created once
(size 80, `UNIFORM | COPY_DST`), the bind group once, and the matrix+eye pair is
written with `Queue::write_buffer` only when it differs from the last uploaded
one. Tests pin `size_of`, the alignment, the field offsets and the column order.
Stage 8 added the eye position (the reference's `u_camera_pos`, which the sheen
needs) and widened the binding's visibility to the fragment stage; see
[WGPU_LIGHTING.md](WGPU_LIGHTING.md) §3.2.

## 8. Coordinate conventions

- **World axes:** right-handed; `+Y` is up; yaw 0 looks toward `−Z`, yaw 90
  toward `+X`; positive pitch looks up (`RenderCamera`).
- **Camera:** `Mat4::look_at_rh(eye, eye + forward, +Y)`.
- **Matrix semantics:** column-major; view-projection is `projection * view`;
  a point is transformed as `M * vec4(position, 1)`.
- **Projection:** `Mat4::perspective_rh_gl`, vertical FOV 60 degrees at the
  480x272 reference aspect (wider drawables expand horizontally,
  `vertical_fov_for_aspect`), near 0.1 m, far 100 m.
- **OpenGL clip depth:** `z_ndc` in `[-1, 1]` (near `-1`, far `+1`), the full
  depth buffer by design.
- **wgpu clip depth:** `z_ndc` in `[0, 1]` (near `0`, far `1`).
- **Y orientation:** wgpu NDC is +Y up like OpenGL; the framebuffer Y flip is
  the viewport transform's job. Stage 5 negates no Y anywhere.
- **Framebuffer/present:** handled entirely by wgpu's viewport; the projection
  is not adjusted for the top-left origin.

## 9. Clip-space conversion

One location: `render::wgpu::world::clip_correction()`.

```text
                    [1  0    0   0]
clip_wgpu = C * clip_gl,  C = [0  1    0   0]
                    [0  0   0.5 0.5]
                    [0  0    0   1]
```

`z' = 0.5 z + 0.5`, `w' = w`, `x`/`y` untouched. `prepare_world_frame` computes
`C * RenderCamera::view_projection(render_size)` and returns it with the neutral
frustum (whose planes are view-space distances, so the correction does not
affect what it culls). Unit tests prove near -> 0, far -> 1, midpoint -> 0.5,
`w` preserved, and no X/Y mirroring.

## 10. Front-face convention

Places geometry is wound so a face's front side is the side its geometric
normal points to, by the right-hand rule over `p0 -> p1 -> p2`
(`render::common`'s documented convention: "every world face is wound to point
out of the solid"). The normal is computed as `cross(p1 - p0, p2 - p0)`, which
is exactly WebGPU's counter-clockwise front-face rule for a right-handed
projection.

Stage 5 therefore uses **`FrontFace::Ccw`** and a unit test projects a floor, a
wall and their reverse sides, asserting the front side is counter-clockwise in
NDC and the reverse side clockwise.

## 11. Cull mode

The **opaque** pipeline uses `Some(Face::Back)` — deliberate, not a default.
The OpenGL reference draws both sides (`glEnable(GL_CULL_FACE)` never appears)
because it shades with `gl_FrontFacing` and the geometry is documented as
"correct for a future cull-enabled path". Stage 5 is that cull-enabled path:
the winding convention above is the contract, and culling is validated against
the canonical views. The only other single-sided case is a surface viewed from
outside the building; the canonical views are interior.

Stage 7's **cut-out and translucent** pipelines use `cull_mode: None`, because
the reference never culls and a window pane is a legitimate two-sided surface:
it must draw from both rooms, with the shading normal flipped by the fragment
stage's `@builtin(front_facing)` exactly as the reference flips it with
`gl_FrontFacing`. Opaque architecture keeps its culling; see
[WGPU_MATERIALS.md](WGPU_MATERIALS.md) §14–§16.
`PLACES_BENCH_NOCULL` only disables the CPU **frustum** test, never pipeline
culling.

## 12. Depth compare

`CompareFunction::LessEqual`, matching the reference's
`glDepthFunc(GL_LEQUAL)`, with `depth_write_enabled: Some(true)` and the depth
attachment cleared to `1.0`. No reversed-Z, no infinite projection, no
logarithmic depth.

## 13. Depth format

`render::wgpu::surface::DEPTH_FORMAT` = `Depth32Float`, the Stage 4 constant
and the same texture Stage 4 recreates on resize. Stage 5 creates no second
depth texture.

## 14. Pipeline target format

The pipeline's colour target is `config.format` — the format the surface
lifecycle selected (`select_surface_format`, preferring
`Bgra8UnormSrgb`/`Rgba8UnormSrgb`, else the first the surface reports). The
pipeline is keyed by format: `ensure_world_pipeline` rebuilds it only when the
configured format changes (a recreated surface can legitimately report a
different one). No format is hard-coded, and the camera binding, buffers and
geometry are format-independent.

## 15. Level upload lifecycle

`WgpuRenderer::set_level` runs once per load:

1. build the neutral mesh (vertex-lit, no lightmap bake);
2. classify each range (`is_stage5_world_range`) and pack the draw set;
3. create one vertex/index buffer pair per chunk and write the vertices;
4. log `[wgpu] world upload: N vertices, M indices, K draws in C chunk(s)`;
5. replace `self.world`, dropping the previous level's GPU buffers;
6. record `LevelBuildStats` (neutral mesh counts plus the upload counters).

A level reload is the same path: buffer ownership moves to the new geometry and
the old buffers are released by the replacement. An empty level (or one whose
architecture is entirely excluded) uploads nothing and still clears/presents.

## 16. Resize interaction

Stage 4 owns surface/depth resizing; Stage 5 adds no resource there. Every
frame computes the projection from the current physical `drawable_size`
(`main` calls `set_drawable_size` each frame), so aspect and FOV follow a
resize, a HiDPI backing-scale change, minimize and restore with no world-buffer
work. The camera uniform is rewritten only when the matrix changed.

## 17. Fragment output

Stage 7 completed the material fragment stages and **Stage 8 the lighting and
sheen**: `fs_main` (opaque and translucent) and `fs_cutout` (alpha-tested)
return the reference's world colour — the display-space product of the sampled
texel, the material vertex colour and the light factor, plus the
view-dependent sheen, converted through the documented sRGB transfer functions
— with the material alpha. The light arrives per vertex (the reference's
vertex-lit mode); an atlas, reflections, fog and emission are later stages. See
[WGPU_MATERIALS.md](WGPU_MATERIALS.md) §11, §14–§16 and
[WGPU_LIGHTING.md](WGPU_LIGHTING.md).

## 18. Features intentionally not yet implemented

Lightmap atlases; shadow maps (the reference has none); reflection probes and
planar reflections; reflection sampling; fog; emission and emissive masks;
decals and their pass; props and their mesh cache; fixtures and their luminous
faces; dynamic objects; UI; debug overlays; the frustum-culling benchmark's
indexing/vertex-layout switches (accepted and ignored; the wgpu path always
submits indexed 16-bit geometry). Stage 6 added the texture system, Stage 7
the material system and Stage 8 the baked lighting and sheen; see
[WGPU_TEXTURES.md](WGPU_TEXTURES.md), [WGPU_MATERIALS.md](WGPU_MATERIALS.md) and
[WGPU_LIGHTING.md](WGPU_LIGHTING.md) for what each deliberately excludes.

## 19. Handoff to Stage 6 (delivered)

Stage 6 added texture creation, uploads, mip levels, samplers, filtering, wrap
modes, fallback textures, caching and sRGB interpretation **without
revisiting**:

- the world vertex layout — the tiling UV was already on the GPU at
  `@location(2)`;
- depth format, compare, winding, cull mode and the clip-space correction;
- the camera uniform and binding (group 1 did gain the texture + sampler
  binding);
- level upload, reload and resize;
- the surface/device lifecycle.

One minimal semantic extension made it possible: `WorldDraw` now carries the
range's existing `MaterialIndex`, resolved to the base texture through the
neutral material table. See [WGPU_TEXTURES.md](WGPU_TEXTURES.md).

The surface around an opening is opaque architecture; the pane itself was
excluded until Stage 7, which draws it with its material's alpha in the
cut-out or translucent pass. See [WGPU_MATERIALS.md](WGPU_MATERIALS.md).
