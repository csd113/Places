# Renderer

Places has one renderer: wgpu 30.0.1 on Metal (macOS), Vulkan (Linux) or Direct3D 12 (Windows). It draws the complete frame — the baked-light world, props and dynamic objects, the lightmap atlas and its vertex-lit fallback, reflection probes and the planar mirror, fixture emission, decals, fog, the emissive bloom chain and resolve, and the HUD — behind the engine-facing `Renderer` facade described in [ARCHITECTURE.md](ARCHITECTURE.md). This document is the renderer reference: current contracts plus the recorded measurements that bound them. The renderer was ported from the project's earlier reference implementation; the preserved implementation and parity evidence live in Git (§14).

## 1. Backend policy and lifecycle

### 1.1 Dependencies

| Item | Choice | Why |
|---|---|---|
| wgpu | `30.0.1`, `default-features = false`, features `["std", "wgsl"]` | one native backend per target plus WGSL shader compilation |
| target backends | `metal` (macOS), `vulkan` (Linux), `dx12` (Windows) | the only backend feature compiled in for that target |
| pollster | `1.0` | blocks on `request_adapter`/`request_device` without an async runtime |
| shaders | WGSL via `include_str!` | the world, decal, post and UI shaders ship as source |

The backend set is exactly the one native backend selected at build time; no alternate-API backend is compiled in, so a silent fallback to another API is impossible by construction.

`render::wgpu::surface::NATIVE_BACKEND` is the single backend per target. The instance is created with exactly that backend mask; after adapter discovery the renderer verifies `AdapterInfo::backend` equals the expected one, and a mismatch is a hard error, never a fallback. The adapter request sets `force_fallback_adapter: false` and uses the default power preference.

| Platform | Backend | Verified |
|---|---|---|
| macOS | Metal | runtime on the development host (the adapter line reports `Metal`) |
| Linux | Vulkan | configuration reviewed; release status in [VERIFICATION.md](VERIFICATION.md) |
| Windows | Direct3D 12 | configuration reviewed; release status in [VERIFICATION.md](VERIFICATION.md) |

### 1.2 Initialization order

```text
set SDL3 app metadata -> initialize SDL3 + video -> create the window
(position_centered, resizable, high_pixel_density, optional borderless
fullscreen; SDL_SyncWindow after a boot-time fullscreen request)
        -> create the wgpu Instance with the native backend mask
        -> create the Surface from the window's raw handles
        -> request a compatible hardware Adapter (force_fallback_adapter: false)
        -> verify AdapterInfo::backend == NATIVE_BACKEND  (hard error on mismatch)
        -> query SurfaceCapabilities
        -> request Device + Queue
        -> install the device-lost callback
        -> select format / present mode / alpha mode
        -> configure the surface on the first frame with a nonzero drawable
        -> create the depth target with the same size
```

Destruction is the exact reverse, enforced by field declaration order inside `WgpuRenderer`: the pending frame (if any) is discarded first, then the surface, then the device, queue and instance, with the depth target and the remaining plain state following. The pending frame must precede the surface because discarding it reaches back into the surface's swapchain, and the surface must precede the device/instance so the layer it owns is released before the device it references. The window outlives the renderer (see [ARCHITECTURE.md](ARCHITECTURE.md) §6).

### 1.3 Unsafe surface creation

`render::wgpu::surface::create` copies the window's raw window/display handles with `SurfaceTargetUnsafe::from_display_and_window` and calls `Instance::create_surface_unsafe`. Safe surface creation is not usable here: `Surface<'window>` would borrow the `Window` for the renderer's lifetime while `main` still needs `&mut Window` for `set_size`/`set_fullscreen`. The unsafe block is isolated in one function with a written `SAFETY` argument; the invariant is:

- the renderer is a local in `main` declared **after** the window, so Rust drops it before the window on every path, including error returns;
- SDL's video subsystem and the window live for the whole frame loop, which is the only time the surface is used;
- the renderer never touches the window in `Drop`.

No transmutes, no faked `'static` in the type system, no leaked window and no reliance on undocumented destruction order. The surface is recreated from the same window on loss.

### 1.4 Format, presentation and depth

Surface format is chosen deterministically over the capabilities the surface reports: `Bgra8UnormSrgb`, then `Rgba8UnormSrgb`, then the first reported format. The first two keep the presentation target sRGB, which is where the display-space pipeline performs its single conversion; the third rule means an unusual surface is never blocked by a hard-coded format. If the adapter offers only a linear format the renderer logs one warning, because no shader variant can present the authored display values without the hardware's encode. `SurfaceColorSpace::Auto` is used.

Presentation mode: VSync on (the shipped default) prefers `Fifo`, then `FifoRelaxed`; VSync explicitly off prefers `Immediate`, then `Fifo`; if nothing is supported the first reported mode is used, else `Fifo`. Uncapped/tearing presentation is never enabled by default. `set_swap_interval` reconfigures the surface live when the player changes the setting and reports the interval in force; no SDL swap-interval handle is involved.

Depth: one main depth texture, `Depth32Float` (`render::wgpu::surface::DEPTH_FORMAT`), `RENDER_ATTACHMENT` usage, sized to the configured surface and cleared to `1.0` in the same pass as the colour clear. It is recreated only when the drawable size changes. There are no shadow, lightmap or reflection depth resources.

### 1.5 Drawable size, resize and minimize

The configured size is always the physical pixel size (`window.size_in_pixels()`), never the logical window size. `main` re-queries it every frame and calls `Renderer::set_drawable_size`; the renderer reconfigures the surface and recreates the depth target only when it changed.

```text
drawable size changed -> needs_configure = true (single flag)
next frame:
  size != 0 -> recreate depth if its size changed, Surface::configure,
               clear/present normally
  size == 0 -> skip acquisition entirely, keep the event loop alive
```

A zero-sized drawable (minimized or hidden window) is not a fatal error and not a busy loop: `main` sleeps ~16 ms on those frames and the renderer configures nothing until a valid size returns. Restore is automatic — no restart, no device recreation. Under Low the scene target follows the new drawable (§12); no world buffer work is involved.

### 1.6 Error handling

Surface acquisition (`Surface::get_current_texture`) is mapped as follows:

| Result | Action |
|---|---|
| `Success`, `Suboptimal` | draw into the acquired texture |
| `Timeout` | skip the frame, one warning per process |
| `Occluded` | skip the frame silently |
| `Outdated` | reconfigure once, retry once (then skip) |
| `Lost` | recreate the surface from the SDL window on the next present, reconfigure, resume |
| `Validation` | fatal: report and stop |

At most one recovery attempt is made per frame; there is no retry loop. Reconfiguring drops any acquired-but-unpresented frame first, because wgpu forbids configuring a surface while one of its textures is alive.

Device loss: `Device::set_device_lost_callback` records the reason on a shared slot; the frame loop checks it before issuing work, reports the first fatal error once (`[wgpu] fatal device error: ...`), skips all later GPU work, and stops through the normal shutdown path with a non-zero exit. Validation errors are not suppressed or captured globally: wgpu's default handler still makes them visible during development.

## 2. Frame structure

```text
render_scene:
  ensure depth / pipelines / post targets / planar target
  select planar plane (Full only) + nearest probe
  planar capture (mirror ranges excluded, modes zeroed, capture environment)
  update material reflection modes + environment uniforms + cameras
  encode:
    scene pass        -> raw scene target + depth  (static, props, dynamics, decals)
    emissive pass     -> raw emissive target       (when an emissive draw survived)
    blur x2           -> quarter-size raw targets
    resolve/present   -> raw presented target
  submit
render_ui:
  UI blends into the raw presented target (the drawable in both profiles)
  presented copied to the sRGB surface with one encode
present: queue.present (or the acquired frame dropped under NOSWAP)
capture: re-render the chain into presented, replay the last UI list, copy to
         the capture texture and read it back
```

| Target | Full | Low |
|---|---|---|
| scene | the drawable | ≤480 px wide, aspect preserved |
| presented / resolve | the drawable | the drawable |
| planar reflection | half the render size, Full only | not allocated |
| probes | up to two 64-texel cubemaps | up to two 32-texel cubemaps |
| bloom/emissive | quarter-size raw targets | quarter-size raw targets |

The resolve and HUD therefore always run at the default-framebuffer resolution; only the 3D scene is profile-sized. All post targets are recreated only when the size or profile changes, and no pipeline, texture, buffer or bind group is created per frame. The backend consumes the neutral batch classification — opaque, alpha cut-out and translucent (sorted back to front) — and the material, emission, lightmap, sheen, reflection and fog terms the neutral tables resolved; `set_level` uploads the level once, and no draw list crosses the facade while no GPU type crosses back.

If the offscreen targets cannot be created (a first frame, or a failed target), the renderer draws straight into the surface framebuffer and the UI blends on the sRGB surface. That path exists for robustness only; the post path is the normal one.

`capture_default_framebuffer` re-renders the chain into the presented target, replays the last UI list, copies the presented raw image into a raw `Rgba8Unorm` capture texture with no transfer function, and reads it back. The returned `RawImage` is byte-equal to the presented display values (the direct fallback keeps the surface-format path). The capture texture and staging buffer exist for the call only and are released afterwards.

## 3. Colour space

The shipped pipeline has no gamma handling of its own. Every texture uploads as raw `Rgba8Unorm`, sampling returns the authored display-space values, and the world, decal and UI shaders assemble in that same display space. The single conversion is the sRGB surface: the sRGB entry points apply the IEC 61966-2-1 transfer functions once, at the final copy, so the presented byte equals the value the shader computed.

This is deliberate; the shipped artwork, the material tints and every lighting constant were calibrated together in that space. A shader-only "decode both factors, multiply, re-encode" pair is algebraically an identity and cannot change a single multiply; the place a linear pipeline genuinely differs is where the CPU bake **adds** terms (room baseline + fixture pool + doorway blend). Summing those in linear space would darken every fixture pool by roughly 17–28 % on the shipped constants and compress the channel ratios that make the coloured rooms read as coloured. The contract is: everything up to the presented target is display-space, and the sRGB surface performs the one conversion when the surface format is sRGB. A linear-only surface format is warned about and presented without that encode (§1.4).

The world shader has raw and sRGB entry points (`fs_main`/`fs_main_raw`, `fs_cutout`/`fs_cutout_raw`, plus the emissive variants): the raw entry points write the display-space assembly straight to a raw target, the sRGB entry points convert for the surface paths. The unlit bypass keeps a direct sample byte-exact: when the vertex colour is white, the light factor is ≥ 1 and the sheen is zero, the sampled base colour is written unchanged. Alpha never passes through the transfer functions: it is the straight scalar product `texel.a × vertex.a × opacity`.

Clear and background values: raw targets clear to the reference display value `(0.08, 0.08, 0.09)` (`CLEAR_COLOR`); the sRGB surface paths use `CLEAR_COLOR_SRGB`, the linear form of the same value through the same IEC curve, so the presented background is identical. Scene, planar and probe clears all use the raw value.

## 4. World geometry and coordinate conventions

### 4.1 Data flow

```text
LevelDef (src/level.rs)
        |  LoadedLevel { level, materials, ... }             (src/loader.rs)
        v
Renderer::set_level(&LoadedLevel)                           (src/render/facade.rs)
        v
WgpuRenderer::set_level                                     (src/render/wgpu/renderer.rs)
        |  build_level_geometry_timed_with_lightmaps(level, catalog, assets,
        |      materials, LightmapBuildOptions::for_profile(quality, mode), ...)
        v
LevelMesh { ranges: Vec<LevelMeshRange>, ... }              (src/render/common/mesh.rs)
        |  range classification + MeshPacker                (src/render/wgpu/world.rs)
        v
pack_world_ranges(mesh, materials) -> (MeshPacker, Vec<WorldDraw>)
        |  each WorldDraw keeps its range's material key
        v
WgpuWorldGeometry::upload(device, queue, mesh, materials)
        |  one GPU vertex/index buffer pair per 16-bit chunk
        v
WorldTextures::resolve(cache, device, queue, draws, materials, table, quality)
        |  per draw: base texture or the shared fallback
        v
WorldPipeline::encode(pass, geometry, textures, filtering, frustum, cull)
```

The world build asks for the atlas build (`LightmapMode::On`) or the vertex-lit build (`LightmapMode::Off`) from `LightmapBuildOptions::for_profile`, exactly as the lightmaps setting requests. An empty draw set yields no buffers and no draws, which clears/presents safely.

### 4.2 Vertices, indices, buffers

The renderer-neutral `Vertex` (72 bytes) carries the full surface description: `pos`, `color` (material factor × baked light in the vertex-lit build, material factor in the atlas build), `uv` (world-space tiling UV from `tiled_uv`), `normal`, `tangent`, `handedness`, `lightmap` (atlas UV in 16-bit fixed point) and `lightmap_page` (byte; `LIGHTMAP_NONE` = 255 when unlightmapped).

`render::wgpu::world::WorldVertex` is `#[repr(C)]` + `bytemuck::Pod`, 64 bytes with explicit tail padding:

| attribute | location | format | offset |
|---|---|---|---|
| position | 0 | `Float32x3` | 0 |
| normal | 1 | `Float32x3` | 12 |
| uv | 2 | `Float32x2` | 24 |
| color | 3 | `Unorm8x4` | 32 |
| lightmap_uv | 6 | `Unorm16x2` | 36 |
| lightmap_page | 7 | `Float32` | 40 |
| tangent | 4 | `Float32x3` | 44 |
| handedness | 5 | `Float32` | 56 |

One interleaved vertex buffer, stride 64, step mode `Vertex`. `WORLD_VERTEX_STRIDE` and the `WORLD_ATTRIB_*` constants name the contract, and unit tests pin `size_of`, every `offset_of!` and every attribute. The colour is quantised to a byte through the neutral `quantize_unit`; `lightmap_uv` uploads as `Unorm16x2` so the hardware expands it exactly as a normalised 16-bit attribute; `lightmap_page` is the plain page byte carried as a float (`0`/`1` select a page, `255` is `LIGHTMAP_NONE`). There is no light index or material index on the GPU; a material is a draw-level binding.

Indices are `IndexFormat::Uint16`. The neutral `MeshPacker` chunks the draw set so an index never exceeds 65 536 vertices; a range larger than one chunk is split and each placement becomes its own draw, so no base-vertex offset is ever needed, there is no conversion to `Uint32`, and therefore no overflow case. The benchmark's indexing/vertex-layout submission switches are accepted and ignored: the renderer always submits indexed 16-bit geometry.

`WgpuWorldGeometry` owns `Vec<WorldChunk>` (one `vertex_buffer`, one `index_buffer`, counts) and `Vec<WorldDraw>` (`chunk`, `index_start`, `index_count`, `vertex_count`, `bounds`, `kind`, the range's `material` key, its `shine` override and its `pass`). Usage: `VERTEX | COPY_DST` and `INDEX | COPY_DST`. Buffers are persistent across frames and rebuilt only by `set_level`; replacing the geometry releases the previous level's buffers, so a level reload is the same path. An empty level (or one whose architecture is entirely excluded) uploads nothing and still clears/presents.

### 4.3 Camera uniform

```rust
#[repr(C, align(16))]
pub struct CameraUniform {
    pub view_projection: [[f32; 4]; 4],  // offset 0,  64 bytes
    pub position: [f32; 3],              // offset 64, 12 bytes
    pub _padding: f32,                   // offset 76
}                                        // 80 bytes total
```

Column-major, matching WGSL `struct Camera { view_projection: mat4x4<f32>, position: vec3<f32>, _padding: f32 }` at `@group(0) @binding(0)`. The buffer is created once (size 80, `UNIFORM | COPY_DST`), the bind group once, and the matrix + eye pair is written with `Queue::write_buffer` only when it differs from the last uploaded one (`camera_uniform_changed` is a pure, tested predicate). Tests pin `size_of`, the alignment, the field offsets and the column order.

### 4.4 Coordinate conventions

- **World axes:** right-handed; `+Y` is up; yaw 0 looks toward `−Z`, yaw 90 toward `+X`; positive pitch looks up (`RenderCamera`).
- **Camera:** `Mat4::look_at_rh(eye, eye + forward, +Y)`.
- **Matrix semantics:** column-major; view-projection is `projection * view`; a point is transformed as `M * vec4(position, 1)`.
- **Projection:** `Mat4::perspective_rh_gl`, vertical FOV 60 degrees at the 480x272 reference aspect (wider drawables expand horizontally, `vertical_fov_for_aspect`), near 0.1 m, far 100 m.
- **Clip depth:** the projection produces the conventional `[-1, 1]` clip depth, converted to wgpu's `[0, 1]` range with one correction (below).
- **Y orientation:** NDC is +Y up; the framebuffer Y flip is the viewport transform's job, and no Y is negated anywhere in the geometry pipeline.

### 4.5 Clip-space correction

One location: `render::wgpu::world::clip_correction()`.

```text
                    [1  0    0   0]
clip_wgpu = C * clip,  C = [0  1    0   0]
                    [0  0   0.5 0.5]
                    [0  0    0   1]
```

`z' = 0.5 z + 0.5`, `w' = w`, `x`/`y` untouched. `prepare_world_frame` computes `C * RenderCamera::view_projection(render_size)` and returns it with the neutral frustum (whose planes are view-space distances, so the correction does not affect what it culls). Unit tests prove near → 0, far → 1, midpoint → 0.5, `w` preserved, and no X/Y mirroring.

### 4.6 Winding, culling, depth and pipeline format

Places geometry is wound so a face's front side is the side its geometric normal points to, by the right-hand rule over `p0 → p1 → p2` ("every world face is wound to point out of the solid"); the normal is `cross(p1 - p0, p2 - p0)`, exactly the counter-clockwise front-face rule for a right-handed projection.

- The **opaque** pipeline uses `FrontFace::Ccw` and `Some(Face::Back)`, deliberately: the winding convention is the contract, and culling is validated against the canonical views.
- The **cut-out and translucent** pipelines use `cull_mode: None`, because a window pane is a legitimate two-sided surface drawn from both rooms, with the shading normal flipped by the fragment stage's `@builtin(front_facing)`.
- Depth compare is `CompareFunction::LessEqual`; depth writes are on for opaque and cut-out and off for translucent. No reversed-Z, no infinite projection, no logarithmic depth. `PLACES_BENCH_NOCULL` disables the CPU **frustum** test only, never pipeline culling.
- The pipeline's colour target is the configured surface format; `ensure_world_pipeline` rebuilds it only when that format changes (a recreated surface can legitimately report a different one). No format is hard-coded, and the camera binding, buffers and geometry are format-independent.
- Every frame computes the projection from the current physical drawable size, so aspect and FOV follow a resize, a HiDPI backing-scale change, minimize and restore with no world-buffer work. The camera uniform is rewritten only when the matrix or eye moved.

The per-level upload logs `[wgpu] world upload: N vertices, M indices, K draws in C chunk(s)` and records `LevelBuildStats` (neutral mesh counts plus the upload counters).

## 5. Textures and mip handling

### 5.1 Source pipeline

```text
level material id / pack material
        |  (engine)
        v
LoadedLevel.materials : MaterialTable          src/materials/resolve.rs
        |  ResolvedTexture { key, origin, class, image: Rc<RawImage> }
        v
WorldTextures::resolve                         src/render/wgpu/world.rs
        |  resolve_base_texture(draw, MaterialRenderState, MaterialTable)
        v
TextureCache::get_or_upload                    src/render/wgpu/texture.rs
        |  fit_image(image, profile, class)     src/quality.rs
        |  mip chain: halve_image() per level
        v
GpuTexture { texture, view, 2x bind groups, meta }
        |  bound when a run of draws shares it
        v
world.wgsl: textureSample(base_texture, base_sampler, in.uv)
```

The decode is `crate::materials::decode_png`, which normalizes any PNG colour type to 8-bit RGBA and caps each edge at `MAX_TEXTURE_DIMENSION` (1024); the renderer consumes the `Rc<RawImage>` the engine already decoded and never opens a file. The quality fit is `crate::quality::fit_image`, so Full keeps the native sheet and Low box-filters it to the profile budget.

### 5.2 Identity, cache and lifetime

`TextureKey` (`src/render/wgpu/texture.rs`) is: the logical texture id (`core:tex_wallpaper_yellow_01`; pack textures are `pack:<namespace>:<path>`), a semantic (`BaseColorDisplay` or `DataLinear`), a quality class (`Surface`, `FixtureFace`, `DecalSheet`, `Prop`, `EmissionMask`) and a quality profile (`Full`/`Low`). The semantic keeps a base-colour entry from colliding with a later normal-map use of the same source; class and profile capture every input the GPU realization depends on (the quality budget and the fitted dimensions). Nothing in the key is a material instance, draw index or pointer: two surfaces that reference the same texture share one entry, one upload and one pair of bind groups.

`TextureCache` is owned by `WgpuRenderer` and holds the bind group layout, the four shared samplers, two maps and the fallback:

| Map | Contents | Lifetime |
|---|---|---|
| `persistent` | catalog and missing/diagnostic textures | renderer lifetime |
| `level` | `TextureOrigin::Pack` textures | one level (dropped by `begin_level`) |

`release_profile_textures` clears both maps; the loaded level's draws keep their `Arc`s alive until `set_level` replaces them, so a frame between the release and the rebuild still draws valid resources. There is no LRU and no eviction beyond those lifetimes: the shipped catalog holds 38 texture assets, so a renderer-lifetime map is bounded, and a level can only introduce pack textures, which die at the next level. Recorded on Places Demo: a first load uploads each distinct base texture once (26 unique textures over 105 draws, 26 uploads, 0 fallbacks); a reload that references those materials uploads nothing (2 reused textures, 0 uploads, 0 fallbacks); a quality change drops both maps and the next `set_level` re-fits at the new budget (Full fits to 1024, ~145 MB resident; Low to 256, ~9 MB).

Both classes upload raw `Rgba8Unorm` (base colour: authored display values sampled raw; normal maps, masks and data: numeric values never gamma-converted) with `TEXTURE_BINDING | COPY_DST`. No compression, no texture arrays, no view formats and no other format is created; every texture is `TextureDimension::D2`, one sample, one array layer. Because both classes upload raw, filtering, blending and mip selection happen in the same display space the shader assembles in; the sRGB surface is the single conversion point.

### 5.3 Upload, mips and samplers

```rust
queue.write_texture(
    wgpu::TexelCopyTextureInfo { texture, mip_level, origin: ZERO, aspect: All },
    &level_pixels,
    wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(width * 4), // tightly packed, unpadded
        rows_per_image: Some(height),
    },
    wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
);
```

`Queue::write_texture` stages the copy internally and does **not** require the 256-byte row alignment that `CommandEncoder::copy_buffer_to_texture` imposes, so no padding is applied and none is needed. One texture allocation per entry; every mip level is written before the texture is used. `row_bytes(width) = width * 4` for RGBA8; tests pin the 96x64 diagnostic's 384-byte rows (not 256-aligned) as the case that proves no padding assumption slipped in. Uploaded dimensions are exactly the fitted image's dimensions; nothing is forced square or power-of-two, and the fallback's 2x2 sheet is a one-level upload.

Mip policy: `mip_level_count = floor(log2(max(width, height))) + 1`, stopping at 1x1 (1024x1024 → 11 levels, 2x2 → 2, 1x1 → 1, 96x64 → 7, 3x3 → 2). The fallback sheet is the one deliberate exception: exactly one level. No level is ever allocated and left uninitialized: `GpuTexture::upload` writes level 0 and each successive level in the same call, and `TextureMeta` records the allocated count and the resident bytes. CPU mip generation is deterministic and display-space: `halve_image` averages blocks of up to 2x2 source texels rounding to nearest, the output extent floors (`max(1, edge / 2)`) so an odd edge's final unpaired row/column is dropped (3x3 → 1x1 averages the top-left 2x2 block, not all nine texels; a constant image stays constant), alpha is averaged the same way and preserved, and no gamma conversion is applied — a deliberate parity choice, not a claim about ideal filtering. Unit tests cover exact 2x2 averages, odd edges, 1x1 idempotence, constant chains, non-square/odd dimensions and the resident byte count. The measured driver-filter consequence is in §15.

Four shared samplers are created once with the device; every field is set explicitly (`compare` is always `None`, `anisotropy_clamp` always 1, `lod_min_clamp` 0, `lod_max_clamp` 32 so the chain is never cut short):

| Policy | Address U/V/W | Mag | Min | Mip | Used by |
|---|---|---|---|---|---|
| `RepeatLinear` | `Repeat` | `Linear` | `Linear` | `Linear` | base colour under the `linear` setting |
| `RepeatNearest` | `Repeat` | `Nearest` | `Nearest` | `Nearest` | base colour under the `nearest` setting |
| `ClampNearest` | `ClampToEdge` | `Nearest` | `Nearest` | `Nearest` | the fallback sheet, the normal-map fallback, the reflection sampler's nearest mode |
| `ClampLinear` | `ClampToEdge` | `Linear` | `Linear` | `Linear` | fitted clamp sheets (prop model sheets) under `linear`, the reflection sampler |

Places bakes tiling into the vertex UV (`tiled_uv` divides world coordinates by the material's `tile_metres`), so walls, floors and ceilings routinely sample with UVs far beyond one repeat. Base-colour textures therefore **repeat**, and a UV > 1 tiles instead of clamping into a stretched edge; no UV offset or scale is applied at bind time because the tiling is already in the geometry, and applying `tile_metres` again would square it. The fallback sheet clamps, and the diagnostic `core:tex_missing` sheet uses the repeating policy like any other base colour. The player's `texture_filtering` setting is applied live: `set_texture_filtering` only records the mode and each draw binds the matching bind group, so no pixel data is re-uploaded; the load-time diagnostic names the active mode.

### 5.4 Fallback, bind groups and diagnostics

The fallback is the committed `assets/core/textures/white_01.png`. The wgpu module embeds its own copy of the bytes (a backend may not import its sibling) and `src/render/tests.rs` pins the two to identical pixels. It is decoded with the normal decoder, uploaded as a raw `Rgba8Unorm` texture with exactly one mip level, and bound with the clamped nearest sampler; a corrupted install degrades to a generated 2x2 white fill, never a failed start. It is a dedicated resource, not a cache entry, used for an architectural draw whose material key is `MATERIAL_NONE`, a material index outside the resolved table, and the fixture luminous faces, material-less housings and prop-fallback boxes that reach the world pass.

A material whose *authored* texture is missing or fails to decode is a different case: the engine's resolver degrades the whole material to the 64x64 magenta/black diagnostic texture (`core:tex_missing`) and logs the error. The cache uploads that diagnostic like any other texture, and the load-time line reports those draws in its `missing` count so a broken asset is visible, not silent. The white fallback and the magenta diagnostic stay distinct.

Bind groups: group 0 is the camera uniform (`@group(0) @binding(0)`); group 1 is `texture_2d<f32>` and its `sampler` (`@group(1) @binding(0)`/`(1)`). The group 1 layout is created once by `TextureCache` and shared by every world pipeline rebuild, so a changed surface format never invalidates a cached bind group. Each `GpuTexture` creates two bind groups at upload, so the player's filtering setting is a handle swap; the fallback's groups use the clamped nearest sampler. There is no bindless descriptor array, no texture array and no per-frame bind group creation, and the draw loop rebinds group 1 only when a run of draws changes texture. `WorldDraw.material` is the only material state the world draw carries: `resolve_base_texture` follows `MaterialRenderState.texture_slots[material]` into `MaterialTable::textures` for `Floor`/`Ceiling`/`Wall`, and returns `None` for `Light`, `PropFallback` and `Decal` (those index spaces belong to their own paths and must never be read as material slots).

One diagnostic line per level load (never per frame):

```text
[wgpu] textures: 26 unique, 26 uploaded, 79 cache hits, 0 fallbacks,
0 missing of 105 draws (145402504 bytes resident, max edge 1024px, linear filtering)
```

`unique` counts distinct base textures the draw set uses, `uploaded` the GPU uploads this load performed, `cache hits` the lookups answered without uploading, `fallbacks` the draws sampling the fallback sheet and `missing` the draws sampling the diagnostic pattern. `RenderStats::texture_binds` reports the frame's texture bind-group changes. Known limitations, all understood: no anisotropic filtering; the fallback is a solid colour so its clamp/nearest policy is unobservable; a non-sRGB surface format would skip the final encode (§1.4); extreme minification differs by the class in §15.

## 6. Materials and the surface response

### 6.1 Source model and resolution

A level references material ids; the engine resolves them through the catalog (or a level pack) into a `MaterialTable`. The rendering-facing properties are:

| Property | Authored | Default | Consumer |
|---|---|---|---|
| base texture | catalog/pack `texture` | the diagnostic magenta pattern when unresolved | base-colour sample |
| tint | catalog/pack `tint` | `[1,1,1]` | the level build bakes it into the vertex colour |
| `tile_metres` | catalog/pack | 2.0 m | the build's world UVs |
| `shine` | catalog/pack | `DEFAULT_ROUGHNESS = 0.6` internally | sheen, reflection |
| legacy `roughness` | catalog/pack | — | kept verbatim as the internal roughness |
| `specular` / `specular_color` | catalog/pack | none / white | sheen colour |
| `normal_texture`, `normal_strength` | catalog/pack | none / 1.0 (cap 2.0) | material normal |
| `alpha_mode`, `opacity`, `alpha_cutoff` | catalog/pack | opaque / 1.0 / 0.5 | pass classification, alpha |
| `reflection_mode`, `reflection_strength` | catalog/pack | none / 0.45 | reflection eligibility and weight |
| `emissive`, `emissive_intensity`, `emissive_mask` | catalog/pack | none | emission term and masks |

`shine` is the author's glossiness (`0.0` matte, `1.0` extremely glossy); the engine stores its inverse, `roughness = 1 - shine`. The catalog rejects authoring both `shine` and `roughness`; a pack accepts both and lets `shine` win.

`render::common::materials::resolve_surface_material(key, materials, table, response_allowed)` is the one renderer-neutral resolution rule. For a Floor, Ceiling or Wall key it produces:

```rust
pub struct ResolvedSurfaceMaterial {
    pub texture: Option<u16>,       // base-colour texture index, None = fallback
    pub normal: Option<u16>,        // normal-map texture index, None = gated off
    pub normal_strength: f32,
    pub specular: [f32; 3],         // zeroed when the response is gated
    pub roughness: f32,             // the shine override wins
    pub alpha: MaterialAlpha,
    pub reflection: MaterialReflection,
    pub response_enabled: bool,
}
```

The rules, in order: (1) a key without a level material, or a fixture/prop-placeholder/decal key, resolves to `ResolvedSurfaceMaterial::plain()` (fallback texture, no response, opaque, no reflection); (2) the response is live when the material authors a normal map or a sheen **and** the quality profile draws the surface response — a gated profile zeroes `specular` and hides the normal map while albedo, alpha, tint and the reflection *mode* are untouched; (3) `roughness` is the per-surface `SurfaceShine` override's inverse when one is authored, otherwise the material's resolved roughness; (4) `reflection_strength()` is `specular × reflection.strength`, so a gated profile zeroes the reflection weight with the sheen.

### 6.2 Per-surface override precedence

Every override resolves at geometry-build time into the neutral `SurfaceKey { kind, material, shine }`; the draw carries that key's `MaterialIndex` and `SurfaceShine`, and the material cache keys on them.

| Carrier | Slot | Rule |
|---|---|---|
| `defaults.wall/floor/ceiling` + `*_shine` | Wall/Floor/Ceiling | fallback for surfaces with no own material |
| `rooms[].material` + `shine` | Floor | the room's floor cells |
| `rooms[].ceiling_material` + `ceiling_shine` | Ceiling | the room's ceiling |
| `walls[].material` + `shine` | Wall | wall body, sills, headers, reveals |
| `walls[].faces[name]` + `face_shine[name]` | Wall | the face's own shine wins; else the wall's shine applies to faces drawing the wall's own material |
| `walls[].openings[].glass` + `glass_shine` | Wall | the pane (a wall surface) |
| `floor_patches[]` + `shine` | Floor | latest patch wins |
| `floor_regions[]` + `shine` | Floor | regions win over patches |
| `floor_regions[].edge_material` + `edge_shine` | Wall | transition skirts |
| ramp / stair / half wall / column / archway / guardrail / threshold / baseboard `material` + `shine`, plus each piece's end/cap/riser/side/post/reveal fields | Floor or Wall as documented in [MAP_AUTHORING_GUIDE.md](MAP_AUTHORING_GUIDE.md) | the piece's own override wins; a piece with a different material but no own shine keeps that material's default |

Two surfaces with the same material and different shine resolve to different GPU material states; two surfaces with the same `(kind, material, shine)` share one uniform and one pair of bind groups.

### 6.3 Identity, layout and bind groups

```rust
pub struct MaterialKey {
    pub kind: SurfaceKind,
    pub material: MaterialIndex,
    pub shine: Option<SurfaceShine>,
}
```

`material_identities(draws)` assigns each draw its slot in first-use order, and `WorldMaterials` creates one `GpuMaterial` per slot. The key is not a source material id alone: the surface kind separates a fixture luminous face from a wall that happens to share a numeric slot, and a per-surface shine override changes the resolved roughness and therefore always earns its own state. `MaterialIndex::MATERIAL_NONE` is a valid key component (the plain state, fallback texture). Material GPU records are created once per level load and dropped with the level; normal-map textures are owned by the texture cache (renderer-lifetime for catalog assets, level-scoped for pack assets) and kept alive additionally by the material's `Arc`.

`MaterialUniform`, 80 bytes, `#[repr(C)]` + `bytemuck::Pod`; WGSL declares the same field order:

| Offset | Field | Type | Meaning |
|---:|---|---|---|
| 0 | `specular` | `vec3<f32>` | sheen colour |
| 12 | `roughness` | `f32` | `1 - shine`, or the legacy value |
| 16 | `normal_strength` | `f32` | normal-map `xy` scale |
| 20 | `alpha_cutoff` | `f32` | cut-out discard threshold |
| 24 | `opacity` | `f32` | alpha multiplier |
| 28 | `flags` | `u32` | see below |
| 32 | `reflection_strength` | `vec3<f32>` | `specular × authored strength` |
| 44 | `reflection_mode` | `u32` | 0 none, 1 probe, 2 planar |
| 48 | `emission_color` | `vec3<f32>` | `emissive × intensity` |
| 60 | `emission_mask_enabled` | `f32` | whether the emission mask is bound |
| 64 | `emission_vertex` | `f32` | `1` when the vertex colour is the emission (fixture luminous faces) |
| 68 | `emission_scale` | `f32` | animated emission multiplier; `1.0` without an animation |
| 72 | `_padding` | `[f32; 2]` | explicit 16-byte tail padding |

Flags: bit 0 `MATERIAL_FLAG_NORMAL_ENABLED` (the normal map is bound and may be sampled), bit 1 `MATERIAL_FLAG_RESPONSE_ENABLED` (the profile draws the surface response), bit 2 `MATERIAL_FLAG_REFLECTION_ELIGIBLE` (eligible for a reflection). No other property is stored: no lights, no shadows, no lightmap pages, no probe matrices, no reflection textures. Unit tests pin the size, every offset, the flag bits and the WGSL struct order.

| Group | Binding | Resource | Lifetime |
|---:|---|---|---|
| 0 | 0 | camera uniform (view-projection, eye) | renderer |
| 1 | 0 | base-colour texture | texture cache |
| 1 | 1 | base-colour sampler (linear/nearest) | renderer |
| 2 | 0 | material uniform | level |
| 2 | 1 | normal-map texture (or the white fallback) | texture cache |
| 2 | 2 | normal-map sampler | renderer |

Group 2's layout is created once with the device and shared by every pipeline rebuild and material binding. Each `GpuMaterial` creates two bind groups at level load — one per filtering mode — so the player's filtering setting is a handle swap. No bind group, buffer or pipeline is created per frame, and the material uniform is written once at creation. A material without a normal map binds the shared white fallback with the clamped nearest sampler and bit 0 clear; a real normal map follows the player's filtering setting (repeat, mips, linear or nearest).

### 6.4 Normal maps

The WGSL reproduces the reference fragment stage:

```wgsl
sampled = textureSample(normal_texture, normal_sampler, uv).xyz * 2.0 - 1.0;
scaled  = vec3(sampled.x * normal_strength, sampled.y * normal_strength, sampled.z);
normal  = select(-normal, normal, front_facing);          // geometric, flipped for a back face
tangent = normalize(world_tangent - normal * dot(normal, world_tangent));
bitangent = cross(normal, tangent) * handedness;
normal  = normalize(tangent * scaled.x + bitangent * scaled.y + normal * scaled.z);
```

There is **no green-channel flip**; the sign lives in the per-vertex handedness attribute. Strength scales `xy` only, before the final normalization; the default is 1.0 and the cap 2.0 (a pack's parser silently caps at 1.0). The world vertex carries `tangent` and `handedness` taken unchanged from the neutral `Vertex` (computed by `compute_surface_frames` from the geometry's winding and UV derivatives). The normal preparation happens before the tangent Gram-Schmidt, and the alpha passes get `front_facing` from the rasterizer. A declared-but-unresolvable normal map degrades the whole material to the diagnostic pattern with the response cleared.

### 6.5 Alpha modes and passes

Classification is the neutral `batch_pass_for` rule:

| Material | Pass | Pipeline state |
|---|---|---|
| `opaque` (default) | opaque | depth write on, no blend, back-face culled |
| `cutout` | cut-out | depth write on, no blend, `discard` below `cutoff`, two-sided |
| `blend`, `opacity > 0` | translucent | `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`, add, depth test LEQUAL, **depth write off**, two-sided, sorted |
| `blend`, `opacity == 0` | opaque | drawn as a normal opaque surface |

Only Floor/Ceiling/Wall ranges with a level material can be cut-out or translucent; fixtures, prop placeholders and decals are opaque by kind and stay out of the world alpha passes. The alpha formula is the straight `texel.a × vertex.a × opacity`; static vertices carry alpha 1, preserved in the `Unorm8x4` upload.

The cut-out pass is a separate fragment entry point (`fs_cutout`) and pipeline, not a branch in the opaque shader, because a `discard` disables early depth testing for every draw that uses the program. The condition is `alpha < cutoff` (strictly less; equality is kept), with the cutoff from the material (default 0.5); depth writes stay on and blending off. The translucent pipeline uses straight alpha, no colour × alpha in the shader, depth test on, depth writes off, and runs after the opaque and cut-out passes. Its blend state is explicit `SrcAlpha`/`OneMinusSrcAlpha`/`Add` for **both** the colour and alpha channels. Translucent draws are ordered per draw (never per triangle) by the squared distance from the camera to the draw's AABB centre, farthest first, with stable ties preserving the packed draw order. Both alpha passes use `cull_mode: None` with the `front_facing` normal flip, while opaque architecture keeps its deliberate back-face culling. Special glass passes, refraction and reflective glass do not exist.

### 6.6 Fallbacks and reflection eligibility

| Situation | Result |
|---|---|
| no material / `MATERIAL_NONE` | white fallback sheet, plain state (no response, opacity 1, cutoff 0.5) |
| unknown material id or unresolved texture | the engine's magenta diagnostic texture; optional terms are cleared, so the renderer sees a plain opaque state |
| material with no normal map | white fallback bound, fetch gate off, geometric normal |
| declared-but-unresolvable normal map | whole material degrades to the diagnostic, response cleared |
| stale normal index | the fallback is bound with the gate off; never an undefined sample |
| legacy material with no shine | no sheen (`specular = 0`), roughness 0.6, no reflection |
| absent alpha fields | opaque, opacity 1, cutoff 0.5 |
| an empty level | no materials, no draws; diagnostics report zeros |

`MaterialReflection` (mode and strength) is resolved by the neutral layer; the GPU record stores `reflection_mode` and `reflection_strength = specular × authored strength`, and bit 2 of `flags` is set when the result can change a pixel. A profile that gates the response zeroes the reflection weight; the runtime reflection resources and switches are in §8.

## 7. Lighting, lightmaps and shadows

### 7.1 No realtime lights

The renderer has no light selection, no light array, no attenuation curve in a shader and no shadow map. Every fixture contribution is baked once per level load on the CPU by `src/lighting/**`, and the world fragment stage contains exactly one light expression:

```wgsl
light = atlas_sample(when the atlas is on) else vec3(1.0);
light *= light_scale;   // the dynamic-object probe, 1 for static geometry
```

Ambient, baselines and pools exist **only in the CPU bake**. The shader has no ambient, no light position and no attenuation input, so "changing the lighting at runtime" means re-baking.

### 7.2 Light sources

A level authors lights in two places; both become the same neutral `LightSource`:

- `ceiling_lights[]` — a fixture placement (`fixture` catalog id, `x`/`z`, `rotation_degrees`, `brightness`/`intensity`, `color`, `mount`, `y`, `range`, `falloff`, `enabled`, `emission`). The fixture family (`FixtureKind`) decides only the emitting rectangle and the visible mesh: fluorescent panel 0.6 × 0.3 m, round downlight 0.22 × 0.22 m, wall sconce 0.20 × 0.09 m, flush mount 0.16 × 0.16 m.
- `props[].lights[]` — up to eight per prop, in the prop's local frame, with an explicit `shape` (point, rectangle or line), offset, rotation, colour, intensity, range and falloff.

A `LightSource` is a shape (`Point`, `Rect`, `Line` — a line is a thin rect), a world position, a colour, an intensity clamped to `0..=8`, a range clamped to `0.05..=64` m (default 6 m) and a falloff (`Smooth` — the `(1-t)²(1+2t)` cushion — `Linear` or `Constant`). It is active when enabled, with a positive intensity and a non-black colour. There is no maximum active count because nothing is dynamic: the bake visits them all.

### 7.3 The baked equation

Per sample point and channel:

```text
room baseline   = clamp(0.50 · c + 0.10, 0.10, 0.60)
                  c = n / (1 + n), n = ln(1 + (power / area) · 500)
                  power = Σ intensity · sqrt(3.5 / ceiling_height) · colour
local pools     = clamp(0.45, Σ 0.42 · intensity · height_factor · falloff · visibility · colour)
opening blend   = 0.5 · smooth_falloff(d / 6) · (neighbour_baseline - own_baseline)
light           = clamp(baseline + pools + blend, 0.10, 1.0)
```

`AMBIENT_LEVEL = 0.10` is the floor; an unlit room is dark by design. `BASELINE_MAX = 0.60` deliberately leaves the highlight headroom to the pools, because a pool is the only term a static occluder can remove. A room split by opaque internal walls gets one baseline per connected area, and a doorway still blends the two areas through the aperture. Floor interfaces and ceiling bodies isolate storeys, so a fixture cannot light through a slab. Pool visibility is a binary (vertex-lit) or multi-tap soft (atlas) test against the same wall solids, floor interfaces, ceiling bodies and prop-derived boxes the geometry and collision use: Full bakes with two taps per axis and 0.075 m prop-occlusion cells, Low with one tap per axis and 0.15 m. Openings transmit light through the hole they cut (the wall solid is removed); glass and grille panes do not have their own material alpha consulted, so a translucent pane transmits exactly like the opening.

### 7.4 Atlas and vertex-lit storage

- **Atlas** (the default; lightmaps on): architectural vertex colours are the material factor `tint × directional face shade` with no baked light, the atlas carries the per-texel light, and `light` is the atlas sample.
- **Vertex-lit** (`PLACES_NO_LIGHTMAPS=1`, or the atlas fallback): the bake is folded into each vertex colour by `shade(base, light) = clamp(base × light)`, every vertex carries `LIGHTMAP_NONE`, and `light` is exactly `vec3(1.0)`.

The bake is the same CPU code in both modes; only the storage differs, and `LightmapMode::Off` always bakes with `BakeConfig::HARD` (one visibility tap, 0.15 m prop-occlusion cell) whatever the quality profile, which keeps the vertex-lit fallback identical in shape and light to the always-supported path.

The atlas itself: the level build requests `LightmapBuildOptions::for_profile(quality, LightmapMode::On)` when lightmaps are on (the default); the neutral bake, planner, fill and content key are shared with the rest of the engine, and the renderer restores the same `cache/lightmaps/v4-<hash>` entries (measured: cold 68 s, warm 19.8 s in a debug build). Atlas pages are raw `Rgba8Unorm` (the alpha byte is 255 and never read). `WorldVertex` carries `lightmap_uv` as `Unorm16x2` and `lightmap_page` as a plain float. `surface_light()` samples `mix(page0, page1, step(0.5, page))` only when `lightmap_enabled * (1 - step(254.5, page)) > 0.5`, then multiplies by `light_scale`; `LIGHTMAP_NONE` keeps the vertex-lit colour exactly. Sheen, reflection and emission are all scaled by that same light factor, so a dark room darkens them. Profiles: Full bakes at 16 texels/m onto 1024² pages with two taps per axis and 0.075 m prop cells; Low at 9 texels/m onto 512² pages with one tap per axis and 0.15 m cells (both from the neutral `QualityProfile`). A plan/fill failure keeps the neutral build's vertex-lit mesh — only an actual atlas-upload failure rebuilds — and the failure is logged.

### 7.5 The sheen

The only view-dependent light term in the world stage:

```wgsl
vec3 view = normalize(camera_position - world_position);
if (response_enabled) {
    float facing  = clamp(abs(dot(normal, view)), 0.0, 1.0);
    float gloss   = 1.0 - roughness;
    float grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
    float ahead   = pow(facing, mix(1.0, 24.0, gloss)) * gloss;
    sheen = specular * (grazing * 0.55 + ahead * 0.45) * light;
}
```

There is no light direction to place a real highlight; both lobes are scaled by the `light` factor and neither invents a source. `roughness` is `1 - shine` (or the legacy authored roughness, or a per-surface shine override), the normal decode is §6.4, and the master gate is the material's response bit — cleared for a material with neither normal map nor sheen, and for the whole scene under Low.

### 7.6 Shadows

There is no GPU shadow system: no shadow render target, no shadow camera or projection matrix, no depth texture sampled as data, no comparison sampler, no PCF kernel, no caster list and no per-light shadow pass. Every shadow in Places is a surface losing a baked pool: opaque wall solids block pools with every opening kind (door, window, vent) cutting the hole it really cuts; room floors contribute zero-thickness interfaces and ceilings solid bodies, which stops light crossing a storey; each static prop's real (or placeholder) model is ground into oriented boxes that block pools and darken the prop's own contact area; alpha and blend state are never consulted, so a cut-out grille pane and a translucent glass pane transmit light exactly like the opening they fill; trim (baseboards, thresholds) does not block, while structural pieces (half walls, columns, archways, guardrails, stairs) do; dynamic objects cast no baked shadow, by design.

### 7.7 Resources

No lighting resource is created per frame. The only new per-frame upload is the camera uniform (matrix **and** eye), written with `Queue::write_buffer` and skipped entirely while both are unchanged. No light buffer, light array, shadow target, shadow sampler or lighting bind group exists. Probes: at most two cubemaps (6 faces of 64²/32² raw RGBA8) baked at load (12 scene submissions). For a vertex-lit build the bake is `BakeConfig::HARD` under both profiles, so geometry and light are identical and only the response gate differs; for an atlas build the profile selects density, page size, tap count and prop-occlusion cell (§7.4).

## 8. Reflections

Reflections are opt-in per material and weighted by the sheen the material already authors, so a rough or dull surface suppresses its reflection instead of mirroring. They are a player setting (Settings → Graphics → Reflections, plus the `PLACES_NO_REFLECTIONS=1` startup override); turning them off removes the planar pass, the probe bake and the reflection texture binds from the frame without a level reload.

**Probes.** Baked once per level load, after the decal upload, with the full scene body (static, props, dynamics, decals). Face size is 64 texels at Full and 32 at Low; the nearest probe to the visible reflective surface is selected per frame, and a black cube is the fallback when no probe exists. Two conventions are pinned. Face row order: a cube face captured into a render target has its first row at the top, while conventional cube-map sampling expects the captured image bottom-up, so the probe projection negates NDC `y` (storing the bottom-up image) and the capture pipeline uses the reversed front face to compensate for the winding flip. Bake position: the routing's centroid is lifted `+1.2 m` (`PROBE_LIFT_M`). Both are verified by `render::wgpu::reflections::tests::the_cube_round_trip_matches_the_reference_face_convention`, a GPU round-trip test (ignored by default) that captures a world-space quad with the real capture matrices and samples it back, checking layer selection, the `s` axis and the `t` axis.

**Planar mirror.** One plane per frame (the nearest whose reflective bounds survive the cull, Full only), half the render size, its own depth, cleared to the raw clear colour. The capture skips the mirror's own static batches (`material_plane == capture_plane`), without which the deck would fill its own reflection image. Sampling uses the projected `uv`, with the `v` flipped because the capture target's first row is NDC `+y`.

## 9. Props, dynamics, fixtures and emission

**Props / GLB models.** The neutral build's `PropMeshBatch` list is uploaded verbatim: world-space, per-vertex-lit vertices, one draw per primitive, model sheets through the texture cache with clamp wrap and the player's filter. Materials are plain-opaque with the primitive's emission (and its mask); props never use normal maps, alpha modes or reflections.

**Dynamic objects.** One small model-space buffer per model, one group-3 environment per object carrying its model matrix and its baked-light probe (`light_scale`), refreshed by `update_dynamic` only when an object moves. They are opaque, outside the static batches and the bake, and cast no shadow. The washer-drum demonstration is spawned by `set_dynamic_demo` for levels that ship one.

**Fixtures and emission.** Fixture luminous faces are `SurfaceKind::Light` ranges with per-vertex emission; their housings draw the shared white sheet. A fixture's light remains entirely in the CPU bake — the emission term is visual only and never illuminates anything. Material emission (`emissive`, `emissive_intensity`, `emissive_mask`) is independent of environmental illumination, so a surface or fixture face can read fully bright while casting nothing, and a light can cast while nothing glows. A level can make a material's emission `pulse` or `flicker`, deterministically and within a bounded depth; the animation reaches the shader through `emission_scale`.

## 10. Decals

Decals are small local surface markings (signs, floor arrows, warning marks) placed on an existing surface. The pass draws generated atlas cells and external PNG sheets as alpha-cut-out quads over the surface they belong to, with a fixed depth bias and a real geometric offset:

- Generated decals are drawn into one shared atlas by the renderer; external sheets (`"source": "file"`) are decoded once per session and uploaded as their own fitted sheet.
- The sheets are fitted (their UVs never leave the sheet), sampled with mipmaps and alpha-tested at 0.5; the pass runs last in the body so it receives the baked lighting of the room.
- `DECAL_SURFACE_OFFSET_M` displaces every decal 0.2 mm along its surface normal. That is a real geometric separation, sub-pixel at any practical viewing distance, so the base texture cannot win a pixel in the near and mid field no matter how the rasteriser fits its plane equations.
- `DECAL_POLYGON_OFFSET` adds a slope-scaled depth bias in the decal pass, so the far field and grazing angles stay in front of the parent surface after the physical offset is below the depth buffer's resolution.

Both constants are defined once in `src/render/common/decals.rs` and applied in one place, `render::add_decal_quad`, so a decal authored later inherits the fix automatically.

## 11. Fog, post-processing and HUD

**Fog** is a scalar mix in the world shader: a squared-exponential distance term with a height term, applied after emission and before the display conversion. The height contribution is capped at 12 m. Fog state (`color`, `density`, `reference_y`, `height_gain`) comes from the neutral level/atmosphere data and travels in the group-3 environment uniform.

**Bloom and resolve.** The scene is drawn into an offscreen colour+depth target and resolved into the display image by one fullscreen pass. Bloom is drawn from the world's **emissive term alone** — never from brightness — so a brightly lit wall cannot glow. The emissive pass shares the scene's depth (an emissive draw that did not survive produces no pass), followed by a quarter-size two-pass 5-tap blur. The resolve adds bloom, exposure, a tone shoulder that leaves everything below 0.75 untouched, and a subtle grade; it is the only place a scene pixel becomes a display pixel. Bloom is a **player setting** (Settings → Graphics → Bloom, plus the `PLACES_NO_BLOOM=1` startup override), not part of the quality profile, so `Full + Bloom Off` and `Low + Bloom On` are both valid. With Bloom off no emissive or blur pass is submitted and the bloom targets are left allocated but unused; with `Low` plus Bloom off the resolve stage is the identity and presents the scene with the plain copy quad. The resolve and HUD run at the drawable's resolution in both profiles; the scene target is the profile-sized one (§12). The emissive and blur targets remain raw display space.

**HUD.** The renderer-owned UI pass (`ui.wgsl`) draws into the raw presented target over the resolved image, at the drawable's resolution, with depth testing off and straight-alpha blending, so semi-transparent panels blend in display space. The layout is authored against a 480×272 reference canvas and scaled by `UiViewport`; the presented target is copied to the sRGB surface with one encode afterwards.

## 12. Quality profiles

Two runtime profiles use the same assets, ids and level content. The profile is selectable while playing; a change releases the profile textures and rebuilds the level's GPU resources from the level already resident.

| Feature | Full | Low | Source |
|---|---|---|---|
| Atlas: texels/m, page edge, padding | 16, 1024, 2 | 9, 512, 1 | `LightmapConfig::for_profile` |
| Bake taps per axis / prop-occlusion cell | 2 / 0.075 m | 1 / 0.15 m | `QualityProfile::bake_config` |
| Surface response (normal map + sheen + reflection strength) | drawn | gated off | `draws_surface_response` |
| Scene resolution | drawable | ≤ 480 wide, aspect preserved | `scene_target_size` |
| Presented/resolve resolution | drawable | drawable | `target_sizes` |
| Planar reflection | enabled | disabled | `Reflections::set_profile` |
| Probe face edge | 64 | 32 | `probe_face_size` |
| Surface / fixture / decal sheets, masks, prop sheets | 1024 / 1024 / 1024 / 512 / 256 | 256 / 256 / 256 / 128 / 128 | `QualityProfile::budget` |
| Fog, emission and its animation, decals, UI | identical | identical | shared code paths |

Downscaling is a load-time step (`fit_image` → `downscaled_to`) cached with the texture it produced, never a per-frame cost. Low leaves the optional surface response out and renders the 3D scene no wider than the historical 480 px reference width: the same level, the same materials and the same ids, with the optional per-pixel work dropped. One deliberate resource difference: because the response is gated off before resolution, the renderer does not upload a normal-map texture at all on Low, while the rendered policy (geometric normal, no sheen) is identical; a live Low→Full switch releases the profile textures and re-resolves, so the map appears.

## 13. Diagnostics and benchmark hooks

With `PLACES_VERBOSE=1` one startup line names the resolved chain:

```text
[renderer] wgpu | adapter: <name> | backend: Metal | device type: ... |
surface format: ... | present mode: Fifo | alpha mode: Opaque |
depth format: ... | drawable: 1280x720
```

Load-time lines (never per frame) cover the world upload, textures, materials and post targets:

```text
[wgpu] world upload: N vertices, M indices, K draws in C chunk(s)
[wgpu] world pipeline for surface format <format>
[wgpu] textures: ...
[wgpu] materials: ...
[wgpu] post targets: scene 480x270 ... presented 1280x720 ...
```

Recoverable events (surface timeout, surface lost/outdated) are reported at most once per process; a VSync change logs the presentation mode it selected; minimize/restore is silent; there are no per-frame logs.

`BENCH_SUMMARY` reports the last frame the renderer actually submitted: draw calls, total batches, visible and total vertices, texture binds and material changes. A frame that is never submitted (`PLACES_BENCH_NORENDER`, or a surface acquisition that keeps returning `Skip`, e.g. while the window is occluded) reports no draw counts rather than a fabricated number. Count accounting distinguishes per-material bind changes from per-draw material records; draw calls and visible vertices are the comparable numbers.

| Switch | Effect |
|---|---|
| `PLACES_LEVEL`, `PLACES_SPAWN`, `PLACES_CAMERA` | level, spawn and camera selection for a run |
| `PLACES_QUALITY=full\|low` | quality profile for one run |
| `PLACES_NO_LIGHTMAPS=1` | force the vertex-lit build |
| `PLACES_NO_BLOOM=1`, `PLACES_NO_REFLECTIONS=1` | disable one post/reflection stage |
| `PLACES_CAPTURE`, `PLACES_CAPTURE_FRAME` | one-frame PNG capture path |
| `PLACES_VERBOSE=1` | developer telemetry |
| `PLACES_BENCH=1`, `PLACES_BENCH_FRAMES`, `PLACES_BENCH_WARMUP`, `PLACES_BENCH_OUT` | benchmark harness |
| `PLACES_BENCH_NOSWAP`, `PLACES_BENCH_NORENDER`, `PLACES_BENCH_FINISH`, `PLACES_BENCH_NOCULL` | submission diagnostics |
| `PLACES_BENCH_WINDOW_CYCLE=<frame>:resize:<w>x<h>\|minimize\|restore[,...]` | scripted live window lifecycle through the real SDL window |
| `PLACES_BENCH_QUALITY_CYCLE=<frame>:<profile>[,...]` | scripted live quality switches through the normal rebuild path |

## 14. Provenance and recorded parity baselines

The renderer was ported feature-for-feature from the project's former GLES2 implementation, which is preserved in Git at the `renderer-gles2-reference` tag (commit `797370e17aab1409a5de3ea70b9a68f742452`); mainline has no dependency on it, and the historical renderer audit and parity evidence live in that snapshot and in mainline Git history. See [RENDERER_REFERENCE.md](RENDERER_REFERENCE.md) for the map.

The recorded comparison figures below are the current renderer measured against that preserved implementation on macOS/Metal (same asset root, 1280×720, default settings, per-pixel worst RGB channel, 0..255). They bound future regressions; a capture compared between two builds of this renderer can require byte equality instead.

| Metric | Value |
|---|---|
| Canonical 50-view set (25 views × Full/Low): smallest per-view mean | 0.103 (`low/drum`) |
| Canonical set: largest per-view mean | 0.854 (`high/pool_entry`) |
| Canonical set: mean of the 50 view means | 0.419 |
| Canonical set: largest share of pixels over 8 | 0.213 % (`high/office`) |
| Canonical set: largest single channel difference | 163 (`high/pool_wide`) |
| Views improved by more than 0.02 mean | 39/50 |
| Views regressed by more than 0.02 mean | 0/50 |
| Expanded campaign (40 views × Full/Low) | 80/80 captured; per-view means 0.000–0.973; hottest 80×45 block 0–3/255 |
| Lightmap atlas pages | byte-identical between the two renderers, Full and Low |
| A/B contribution correlation: planar / lightmap / probe | 0.9994 / 0.991–0.993 / 0.993–0.994 |
| A/B contribution magnitudes (mean, max): planar / lightmap / probe / bloom | 3.931 vs 3.922, 32/32 · 5.054 vs 5.040, 49/49 · 1.062 vs 1.057, 5/5 · 0.067 vs 0.063 |
| Places Demo draw set (canonical frame) | 118 static + 37 prop + 1 dynamic + 3 decal draws, plus the UI pass; static upload 6,411 vertices / 10,470 indices / 118 draws |
| Lifecycle: texture residency Full / Low | 188,743,640 B / 15,728,600 B, stable across the scripted Full↔Low rebuilds |

The parity evidence for the preserved implementation is reproducible from the tag worktree; the procedure is in [VERIFICATION.md](VERIFICATION.md).

## 15. Accepted behaviour and differences

The still-true bounded differences of the current renderer, all measured against the preserved implementation on the same asset root:

- **Minified texture sampling.** Flat, near-1:1 surfaces are byte-identical; the residue appears where textures are minified. The deterministic CPU box mip chain rounds half-up; a driver-generated chain rounds implementation-defined. Measured on an isolated capture: truncation mean 0.700 with a −0.38 signed bias, half-up 0.509 with +0.24, half-even 0.419 with +0.09. Half-even fits that one driver more closely, but choosing a rounding rule for every backend from one measurement would not be backend-neutral, so the documented deterministic half-up chain is kept.
- **High-contrast raster edges.** One-to-six-pixel coverage ties at silhouettes (the largest is a near-horizontal window-frame edge, bounded by the Low upscale). The canonical maximum single-pixel difference of 163 comes from this class.
- **Bloom halos on emissive faces.** The canonical population above the 8/255 threshold includes soft emissive/bloom brightness-envelope differences at fluorescent panels and pool lights; they are not displaced geometry or missing features.
- **A non-sRGB surface format** would present without the final encode (§1.4); the verification host selects `Bgra8UnormSrgb`.
- **The direct-to-surface fallback** blends the UI on the sRGB surface; it is a first-frame/failure path only.

## 16. Tests and regression coverage

The renderer's contracts are covered by in-crate tests, most of which run without a GPU because they pin CPU-side layouts, mirrors of the shader maths and pipeline state:

- **Geometry:** the 72-byte neutral vertex, the 64-byte `WorldVertex` and every offset, the clip correction (near → 0, far → 1, midpoint → 0.5, `w` preserved, no mirroring), winding direction, culling, depth compare.
- **Camera:** the 80-byte uniform layout, the write predicate, the sRGB clear colours against the reference display value.
- **Textures:** key semantics, exact 2x2 and odd-edge mip averages, 1x1 idempotence, constant chains, resident bytes, sampler policies, wrap selection, the fallback's committed pixels, raw display-space sampling.
- **Materials:** resolution rules, the 80-byte uniform and its flags, the display-space colour maths against every authored texel byte, normal decode, alpha classification, blend state, translucent ordering, fallbacks.
- **Lighting:** the sheen equation (CPU mirror), the display-space assembly order, the unlit bypass conditions, the vertex-lit build's byte-for-byte mesh, the lightmap CPU mirror of `surface_light`, the `needs_upload_fallback` rule.
- **Reflections:** the six face directions/ups, the 90° projection with the Y flip, the planar mirror composition, `+1.2 m` bake position, nearest probe/plane rules, and the ignored GPU cube round-trip.
- **Post/UI:** blur kernel and step, target sizes (scene = profile, presented = drawable), resolve maths, the single-conversion contract, ortho corners, viewport maths, blend factors.
- **Integration:** the world pipeline variants and their states, emissive flag propagation, material reflection-mode rules, live window and quality cycle parsing (`PLACES_BENCH_WINDOW_CYCLE`, `PLACES_BENCH_QUALITY_CYCLE`).

Five diagnostics are intentionally ignored by default: two GPU measurements (requiring an adapter) and three developer measurement/reporting tools. They run explicitly with `cargo test --all-features --bin places -- --ignored`; see [VERIFICATION.md](VERIFICATION.md).
