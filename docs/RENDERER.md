# Renderer

Places has one renderer: wgpu 30.0.1 on Metal (macOS), Vulkan (Linux) or Direct3D 12 (Windows). It draws the complete frame — the baked-light world, props, dynamic objects (including door leaves), animated characters, translucent water surfaces and steam effect billboards, the lightmap atlas and its vertex-lit fallback, reflection probes and the planar mirror, fixture emission, decals, fog, the emissive bloom chain and resolve, and the HUD — behind the engine-facing `Renderer` facade described in [ARCHITECTURE.md](ARCHITECTURE.md). This document is the renderer reference: current contracts plus the recorded measurements that bound them. The renderer was ported from the project's earlier reference implementation; the preserved implementation and parity evidence live in Git (§14).

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

A zero-sized drawable (minimized or hidden window) is not a fatal error and not a busy loop: `main` sleeps ~16 ms on those frames and the renderer configures nothing until a valid size returns. Restore is automatic — no restart, no device recreation. Under Low and Medium the scene target follows the new drawable (§12); no world buffer work is involved.

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
  select planar plane (Medium and High only) + nearest probe
  planar capture (mirror ranges excluded, modes zeroed, capture environment)
  update material reflection modes + environment uniforms + cameras
  encode:
    scene pass        -> raw scene target + depth  (static, props, dynamics, characters, decals, then effects)
    emissive pass     -> raw emissive target       (when an emissive draw survived)
    blur x2           -> quarter-size raw targets
    resolve/present   -> raw presented target
  submit
render_ui:
  UI blends into the raw presented target (the drawable at every level)
  presented copied to the sRGB surface with one encode
present: queue.present (or the acquired frame dropped under NOSWAP)
capture: re-render the chain into presented, replay the last UI list, copy to
         the capture texture and read it back
```

| Target | High | Medium | Low |
|---|---|---|---|
| scene | the drawable | half the drawable, aspect preserved | ≤480 px wide, aspect preserved |
| presented / resolve | the drawable | the drawable | the drawable |
| planar reflection | half the render size | half the render size | not allocated |
| probes | up to two 64-texel cubemaps | up to two 48-texel cubemaps | up to two 32-texel cubemaps |
| bloom/emissive | quarter-size raw targets | quarter-size raw targets | quarter-size raw targets |

The resolve and HUD therefore always run at the default-framebuffer resolution; only the 3D scene is level-sized. All post targets are recreated only when the size or level changes, and no pipeline, texture, buffer or bind group is created per frame. The backend consumes the neutral batch classification — opaque, alpha cut-out and translucent (sorted back to front) — and the material, emission, lightmap, sheen, reflection and fog terms the neutral tables resolved; prepared installation uploads the level once, and no draw list crosses the facade while no GPU type crosses back.

If the offscreen targets cannot be created (a first frame, or a failed target), the renderer draws straight into the surface framebuffer and the UI blends on the sRGB surface. That path exists for robustness only; the post path is the normal one.

`capture_default_framebuffer` re-renders the chain into the presented target, replays the last UI list, copies the presented raw image into a raw `Rgba8Unorm` capture texture with no transfer function, and reads it back. The returned `RawImage` is byte-equal to the presented display values (the direct fallback keeps the surface-format path). The capture texture and staging buffer exist for the call only and are released afterwards.

## 3. Colour space

The shipped pipeline has no gamma handling of its own. Every texture uploads as raw `Rgba8Unorm`, sampling returns the authored display-space values, and the world, decal and UI shaders assemble in that same display space. The single conversion is the sRGB surface: the sRGB entry points apply the IEC 61966-2-1 transfer functions once, at the final copy, so the presented byte equals the value the shader computed.

This is deliberate; the shipped artwork, the material tints and every lighting constant were calibrated together in that space. A shader-only "decode both factors, multiply, re-encode" pair is algebraically an identity and cannot change a single multiply; the place a linear pipeline genuinely differs is where the CPU bake **adds** terms (room baseline + directional pool + bounce fill + doorway blend). Summing those in linear space would darken every fixture pool by roughly 17–28 % on the shipped constants and compress the channel ratios that make the coloured rooms read as coloured. The contract is: everything up to the presented target is display-space, and the sRGB surface performs the one conversion when the surface format is sRGB. A linear-only surface format is warned about and presented without that encode (§1.4).

The world shader has raw and sRGB entry points (`fs_main`/`fs_main_raw`, `fs_cutout`/`fs_cutout_raw`, plus the emissive variants): the raw entry points write the display-space assembly straight to a raw target, the sRGB entry points convert for the surface paths. The unlit bypass keeps a direct sample byte-exact: when the vertex colour is white, the light factor is ≥ 1 and the sheen is zero, the sampled base colour is written unchanged. Alpha never passes through the transfer functions: it is the straight scalar product `texel.a × vertex.a × opacity`.

Clear and background values: raw targets clear to the reference display value `(0.08, 0.08, 0.09)` (`CLEAR_COLOR`); the sRGB surface paths use `CLEAR_COLOR_SRGB`, the linear form of the same value through the same IEC curve, so the presented background is identical. Scene, planar and probe clears all use the raw value.

## 4. World geometry and coordinate conventions

### 4.1 Data flow

```text
LevelDef (src/level.rs)
        |  LoadedLevel { level, materials, ... }             (src/loader.rs)
        v
loading::Loader worker                                    (src/loading.rs)
        |  resolve assets, prepare geometry/lighting, restore or fill atlas
        |  reuse matching immutable build from bounded worker cache
        v
PreparedWorld { build: Arc<LevelBuild>, collision, characters, ... }
        |  nonblocking completion poll, reject superseded generations
        v
Renderer::install_prepared / advance_prepared_install      (src/render/facade.rs)
        |  staged GPU work on the main thread
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

The world build asks for the atlas build (`LightmapMode::On`) or the vertex-lit build (`LightmapMode::Off`) from `LightmapBuildOptions::for_lightmaps`, exactly as the Lightmaps setting requests. An empty draw set yields no buffers and no draws, which clears/presents safely.

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

`WgpuWorldGeometry` owns `Vec<WorldChunk>` (one `vertex_buffer`, one `index_buffer`, counts) and `Vec<WorldDraw>` (`chunk`, `index_start`, `index_count`, `vertex_count`, `bounds`, `kind`, the range's `material` key, its `shine` override and its `pass`). Usage: `VERTEX | COPY_DST` and `INDEX | COPY_DST`. Buffers are persistent across frames and replaced only when a prepared installation commits; replacing the geometry releases the previous level's buffers, so a level reload is the same path. An empty level (or one whose architecture is entirely excluded) uploads nothing and still clears/presents.

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
        |  ResolvedTexture { key, origin, class, image: Arc<RawImage> }
        v
WorldTextures::resolve                         src/render/wgpu/world.rs
        |  resolve_base_texture(draw, MaterialRenderState, MaterialTable)
        v
TextureCache::get_or_upload                    src/render/wgpu/texture.rs
        |  fit_image(image, level, class)       src/quality.rs
        |  mip chain: halve_image() per level
        v
GpuTexture { texture, view, 3x bind groups, meta }
        |  bound when a run of draws shares it
        v
world.wgsl: textureSample(base_texture, base_sampler, in.uv)
```

The decode is `crate::materials::decode_png`, which normalizes any PNG colour type to 8-bit RGBA and caps each edge at `MAX_TEXTURE_DIMENSION` (1024); the renderer consumes the `Arc<RawImage>` the engine already decoded and never opens a file. The quality fit is `crate::quality::fit_image`, so High keeps the native sheet while Medium and Low box-filter it to the level's budget for its class.

### 5.2 Identity, cache and lifetime

`TextureKey` (`src/render/wgpu/texture.rs`) is: the source identity plus a content revision (`#png-v1-<hash>` for encoded PNGs, `#rgba-v1-<hash>` for decoded model images), a semantic (`BaseColorDisplay` or `DataLinear`), a quality class (`Surface`, `FixtureFace`, `DecalSheet`, `Prop`, `EmissionMask`) and a quality level (`Low`/`Medium`/`High`). The semantic keeps a base-colour entry from colliding with a later normal-map use of the same source; class and level capture every input the GPU realization depends on (the quality budget and the fitted dimensions). Nothing in the key is a material instance, draw index or pointer: two surfaces that reference the same texture share one entry, one upload and one set of filtering bind groups.

`TextureCache` is owned by `WgpuRenderer` and holds the bind group layout, the nine shared samplers, two maps and the fallback:

| Map | Contents | Lifetime |
|---|---|---|
| `persistent` | catalog, model and missing/diagnostic textures | prior-upload retention bounded at the next upload |
| `level` | `TextureOrigin::Pack` textures | one level (dropped by `begin_level`) |

`release_profile_textures` clears both maps. `begin_level` clears pack entries
and trims prior persistent entries to 256 entries and 256 MiB of mip texel
storage, evicting the largest optional entries first. This is a retention
budget, not a cap on the resources a valid active level can use: no eviction
happens between that upload's sheets. Old content revisions are removed when
the same source changes. Active and pending worlds keep their own `Arc`s, so
cache eviction cannot invalidate their draws. The CPU image cache applies the
same entry/decoded-byte retention limits before a level resolves its materials;
prop parsing retains the current request's model dependency set.

`WorldTextures::resolve` prepares distinct misses in parallel (quality fit and
CPU mip chains), then creates and writes GPU textures serially on the caller's
thread. Matching content, semantic, class and quality reuse resident entries.


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

Nine shared samplers are created once with the device; every field is set explicitly (`compare` is always `None`, `lod_min_clamp` 0, `lod_max_clamp` 32 so the chain is never cut short). The six world policies all filter `Linear`/`Linear`/`Linear` — every level of the player's **Texture Filtering** setting is trilinear, no level disables mips and none falls back to point sampling — and differ only in the requested anisotropy (Low 4x, Medium 8x, High 16x):

| Policy | Address U/V/W | Mag | Min | Mip | Anisotropy | Used by |
|---|---|---|---|---|---|---|
| `RepeatLow` / `RepeatMedium` / `RepeatHigh` | `Repeat` | `Linear` | `Linear` | `Linear` | 4 / 8 / 16 | repeating world base colour at the active level |
| `ClampLow` / `ClampMedium` / `ClampHigh` | `ClampToEdge` | `Linear` | `Linear` | `Linear` | 4 / 8 / 16 | clamped fitted sheets (prop model sheets, fixture faces, emissive masks) at the active level |
| `RepeatNearest` | `Repeat` | `Nearest` | `Nearest` | `Nearest` | 1 | retained point-sample policy; no filtering level selects it |
| `ClampNearest` | `ClampToEdge` | `Nearest` | `Nearest` | `Nearest` | 1 | the fallback sheet and the UI — independent of the setting |
| `ClampLinear` | `ClampToEdge` | `Linear` | `Linear` | `Linear` | 1 | the lightmap atlas pages and the reflection targets (probe/planar) — independent of the setting |

Anisotropy above 1 requires all three filters to be linear, which the world presets satisfy by construction; wgpu-core clamps the request to 16 and, on an adapter without `DownlevelFlags::ANISOTROPIC_FILTERING`, silently to 1. The renderer queries that flag once at startup and creates the world presets through `SamplerPolicy::effective_descriptor`: when the capability is absent the three levels keep their trilinear `Linear`/`Linear`/`Linear` filtering and clamp to 1x, and the `PLACES_VERBOSE` startup line states which happened. No device feature is requested. Ordinary world/material/decal textures keep the complete CPU mip chain (§5.3 mip policy), which is exactly what anisotropic filtering requires.

Places bakes tiling into the vertex UV (`tiled_uv` divides world coordinates by the material's `tile_metres`), so walls, floors and ceilings routinely sample with UVs far beyond one repeat. Base-colour textures therefore **repeat**, and a UV > 1 tiles instead of clamping into a stretched edge; no UV offset or scale is applied at bind time because the tiling is already in the geometry, and applying `tile_metres` again would square it. The fallback sheet clamps, and the diagnostic `core:tex_missing` sheet uses the repeating policy like any other base colour. The player's `texture_filtering` setting is applied live: `set_texture_filtering` only records the level and each draw binds the matching bind group, so no pixel data is re-uploaded and the environment binding is not rebuilt (the lightmap sampler is a fixed clamped linear policy and does not follow this setting); the load-time diagnostic names the active level.

### 5.4 Fallback, bind groups and diagnostics

The fallback is the committed `assets/core/textures/white_01.png`. The wgpu module embeds its own copy of the bytes (a backend may not import its sibling) and `src/render/tests.rs` pins the two to identical pixels. It is decoded with the normal decoder, uploaded as a raw `Rgba8Unorm` texture with exactly one mip level, and bound with the clamped nearest sampler; a corrupted install degrades to a generated 2x2 white fill, never a failed start. It is a dedicated resource, not a cache entry, used for an architectural draw whose material key is `MATERIAL_NONE`, a material index outside the resolved table, and the fixture luminous faces, material-less housings and prop-fallback boxes that reach the world pass.

A material whose *authored* texture is missing or fails to decode is a different case: the engine's resolver degrades the whole material to the 64x64 magenta/black diagnostic texture (`core:tex_missing`) and logs the error. The cache uploads that diagnostic like any other texture, and the load-time line reports those draws in its `missing` count so a broken asset is visible, not silent. The white fallback and the magenta diagnostic stay distinct.

Bind groups: group 0 is the camera uniform (`@group(0) @binding(0)`); group 1 is `texture_2d<f32>` and its `sampler` (`@group(1) @binding(0)`/`(1)`). The group 1 layout is created once by `TextureCache` and shared by every world pipeline rebuild, so a changed surface format never invalidates a cached bind group. Each `GpuTexture` creates three bind groups at upload — one per world filtering level — so the player's setting is a handle swap; the fallback's three groups all bind the clamped nearest policy. There is no bindless descriptor array, no texture array and no per-frame bind group creation, and the draw loop rebinds group 1 only when a run of draws changes texture. `WorldDraw.material` is the only material state the world draw carries: `resolve_base_texture` follows `MaterialRenderState.texture_slots[material]` into `MaterialTable::textures` for `Floor`/`Ceiling`/`Wall`, and returns `None` for `Light`, `PropFallback` and `Decal` (those index spaces belong to their own paths and must never be read as material slots).

One diagnostic line per level load (never per frame):

```text
[wgpu] textures: 26 unique, 26 uploaded, 79 cache hits, 0 fallbacks,
0 missing of 105 draws (145402504 bytes resident, max edge 1024px, high filtering)
```

`unique` counts distinct base textures the draw set uses, `uploaded` the GPU uploads this load performed, `cache hits` the lookups answered without uploading, `fallbacks` the draws sampling the fallback sheet and `missing` the draws sampling the diagnostic pattern. `RenderStats::texture_binds` reports the frame's texture bind-group changes. Known limitations, all understood: the fallback is a solid colour so its clamp/nearest policy is unobservable; a non-sRGB surface format would skip the final encode (§1.4); extreme minification differs by the class in §15.

## 6. Materials and the surface response

### 6.1 Source model and resolution

A level references material ids; the engine resolves them through the catalog into a `MaterialTable`. The rendering-facing properties are:

| Property | Authored | Default | Consumer |
|---|---|---|---|
| base texture | catalog `texture` | the diagnostic magenta pattern when unresolved | base-colour sample |
| tint | catalog `tint` | `[1,1,1]` | the level build bakes it into the vertex colour |
| `tile_metres` | catalog | 2.0 m | the build's world UVs |
| `shine` | catalog | `0.4` (`DEFAULT_ROUGHNESS = 0.6` internally) | sheen, reflection |
| `specular` / `specular_color` | catalog | none / white | sheen colour |
| `normal_texture`, `normal_strength` | catalog | none / 1.0 (cap 2.0) | material normal |
| `alpha_mode`, `opacity`, `alpha_cutoff` | catalog | opaque / 1.0 / 0.5 | pass classification, alpha |
| `reflection_mode`, `reflection_strength` | catalog | none / 0.45 | reflection eligibility and weight |
| `emissive`, `emissive_intensity`, `emissive_mask` | catalog | none | emission term and masks |

`shine` is the author's glossiness (`0.0` matte, `1.0` extremely glossy); the
engine stores the shader-facing inverse, `roughness = 1 - shine`.

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

The rules, in order: (1) a key without a level material, or a fixture/prop-placeholder/decal key, resolves to `ResolvedSurfaceMaterial::plain()` (fallback texture, no response, opaque, no reflection); (2) the response is live when the material authors a normal map or a sheen **and** the quality level draws the surface response — a gated level zeroes `specular` and hides the normal map while albedo, alpha, tint and the reflection *mode* are untouched; (3) `roughness` is the per-surface `SurfaceShine` override's inverse when one is authored, otherwise the material's resolved roughness; (4) `reflection_strength()` is `specular × reflection.strength`, so a gated level zeroes the reflection weight with the sheen.

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
| ramp / stair / half wall / column / archway / arc wall / pillar / guardrail / threshold / baseboard `material` + `shine`, plus each piece's end/cap/riser/side/post/reveal/inner/outer fields | Floor or Wall as documented in [MAP_AUTHORING_GUIDE.md](MAP_AUTHORING_GUIDE.md) | the piece's own override wins; a piece with a different material but no own shine keeps that material's default |

Two surfaces with the same material and different shine resolve to different GPU material states; two surfaces with the same `(kind, material, shine)` share one uniform and one pair of bind groups.

### 6.3 Identity, layout and bind groups

```rust
pub struct MaterialKey {
    pub kind: SurfaceKind,
    pub material: MaterialIndex,
    pub shine: Option<SurfaceShine>,
}
```

`material_identities(draws, routing)` assigns each draw its slot in first-use order, and `WorldMaterials` creates one `GpuMaterial` per slot. The key is not a source material id alone: the surface kind separates a fixture luminous face from a wall that happens to share a numeric slot, a per-surface shine override changes the resolved roughness and therefore always earns its own state, and a planar range's mirror plane is part of the key, so one material used on two floors keeps two reflection states (and two capture-exclusion identities). `MaterialIndex::MATERIAL_NONE` is a valid key component (the plain state, fallback texture). Material GPU records are created once per level load and dropped with the level; normal-map textures are owned by the texture cache (renderer-lifetime for catalog assets, level-scoped for pack assets) and kept alive additionally by the material's `Arc`.

`MaterialUniform`, 80 bytes, `#[repr(C)]` + `bytemuck::Pod`; WGSL declares the same field order:

| Offset | Field | Type | Meaning |
|---:|---|---|---|
| 0 | `specular` | `vec3<f32>` | sheen colour |
| 12 | `roughness` | `f32` | `1 - shine` |
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

Flags: bit 0 `MATERIAL_FLAG_NORMAL_ENABLED` (the normal map is bound and may be sampled), bit 1 `MATERIAL_FLAG_RESPONSE_ENABLED` (the level draws the surface response), bit 2 `MATERIAL_FLAG_REFLECTION_ELIGIBLE` (eligible for a reflection). No other property is stored: no lights, no shadows, no lightmap pages, no probe matrices, no reflection textures. Unit tests pin the size, every offset, the flag bits and the WGSL struct order.

| Group | Binding | Resource | Lifetime |
|---:|---|---|---|
| 0 | 0 | camera uniform (view-projection, eye) | renderer |
| 1 | 0 | base-colour texture | texture cache |
| 1 | 1 | base-colour sampler (Low/Medium/High world preset) | renderer |
| 2 | 0 | material uniform | level |
| 2 | 1 | normal-map texture (or the white fallback) | texture cache |
| 2 | 2 | normal-map sampler | renderer |

Group 2's layout is created once with the device and shared by every pipeline rebuild and material binding. Each `GpuMaterial` creates three bind groups at level load — one per Texture Filtering level — so the player's setting is a handle swap. No bind group, buffer or pipeline is created per frame, and the material uniform is written once at creation. A material without a normal map or emission mask binds the shared white fallback with the clamped nearest sampler and bit 0 clear; a real map follows the player's filtering setting (repeat, mips, trilinear, 4x/8x/16x anisotropy).

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
| material with no shine | no sheen (`specular = 0`), roughness 0.6, no reflection |
| absent alpha fields | opaque, opacity 1, cutoff 0.5 |
| an empty level | no materials, no draws; diagnostics report zeros |

`MaterialReflection` (mode and strength) is resolved by the neutral layer; the GPU record stores `reflection_mode` and `reflection_strength = specular × authored strength`, and bit 2 of `flags` is set when the result can change a pixel. A level that gates the response zeroes the reflection weight; the runtime reflection resources and switches are in §8.

## 7. Lighting, lightmaps and shadows

### 7.1 Prepared lighting, no realtime lights

The renderer has no light selection, no light array, no attenuation curve in a
shader and no shadow map. Every fixture contribution is solved **offline** by
`places-compile` (the transport solver in `src/lighting/transport.rs`) and
packaged; the player decodes prepared data and samples it. The world fragment
stage contains exactly one light expression:

```wgsl
light = prepared_light(when the atlas is on) else vec3(1.0);
light *= light_scale;   // the prepared probe field, 1 for static geometry
```

The prepared light is linear HDR and directional: each texel stores an
irradiance term `I` and the direction-moment vector `g` the compiler solves
(§7.4), and the shader reconstructs, with `k = I.r + I.g + I.b`,

```wgsl
light(n) = max(0, I + (I / max(k, 1e-6)) * (2 * max(0, dot(g, n)) - length(g)));
```

then runs the display tone map (`soft_clip`: values up to 0.8 pass through;
brighter values compress with a C1 exponential shoulder) before multiplying
the surface. The directional term is the calibrated sharp cosine evaluated
from the *interpolated* moment vector: exact for one shared direction
(`2 * I * max(0, cos)`), zero for a receiver facing away, and collapsing
smoothly to the isotropic mean where opposing lights cancel the moment. It
integrates to `1/4` of the peak over the sphere, so the reconstructed field
mean stays exactly `I`. The stored light is albedo-free: the fragment stage
multiplies the base colour exactly once.

**What is still runtime.** Sampling the prepared data (including the probe
field and the switchable-light mask), the dynamic-object probe refresh, the
camera and UI, doors/interactions, planar mirrors and particle emission. No
player path bakes static light, plans charts, captures probes or generates
static navigation.

### 7.1.1 What the compiler solves

`src/lighting/transport.rs` builds a deterministic BVH over the prepared
triangles (walls, floors, ceilings, architecture, prop fallback boxes; decals
and fixture housings are excluded, so a fixture cannot shadow itself), resolves
the level's `LightSource`s as point/rect/line emitters, and then:

1. **Direct**: each light is sampled on its authored shape with a per-axis tap
   pattern, every tap shadow-tested; the visible tap fraction is the soft
   shadow. Ceiling fixtures keep the historical horizontal-reach falloff, so a
   tall chamber's floor stays lit; the receiver's normal supplies the incidence
   in the reconstruction.
2. **Bounce**: uniform-hemisphere ray samples per texel read the previous pass's
   solved surfaces through a side/surface-aware cache. Each cell retains a
   representative for every triangle occupying it, and answers only for the
   hit triangle, so neighboring charts cannot evict one another or transfer
   light through a wall. A shared deterministic angular sequence per pass
   makes the samples independent of chart ordering. One pass is one diffuse
   bounce; Medium runs one, Full two, and the
   estimator is unbiassed (no truncated point-light list).
3. **Denoise**: a chart-space luma-guided 3x3 pass (gated on the smooth
   irradiance luminance, so direction noise cannot lock itself in) removes
   gather noise without crossing a real lighting edge, then the chart gutters
   are dilated.
4. **Transmission.** Translucent architecture does not block the solve: an
   authored window pane, vent grille or screen (`alpha_mode: blend` or
   `cutout`) is a transmissive triangle and a shadow or bounce ray passes
   straight through it, exactly as the historical model treated the opening;
   an opaque material (including solid glass) still blocks. A water volume's
   drawn surface transmits the same way, so a fixture above a pool reaches the
   basin. The submerged path then attenuates the receiver's direct and bounce
   light with a bounded per-channel Beer–Lambert term
   (`WATER_EXTINCTION_PER_M`, red absorbed most) over the vertical distance
   below the surface, applied once per receiver; the authored fill target is
   scaled by the same factors so the physical solve and the fill describe one
   water tint.
5. **Authored fill floor.** Every architectural receiver, including ceilings,
   uses the same continuous response after the physical solve. The authored
   baseline above ambient is multiplied by the maximum range/falloff support
   of always-on emitters at that position: directional fixtures use horizontal
   shape distance and point sources use three-dimensional distance. Thus the
   fill follows actual fixture pools even on a tall chamber's ceiling, instead
   of overwhelming small indirect gradients with a uniform target. Given
   physical light `L` and this spatial target `T`, the result is
   `L + T * max(1 - L / (4*T), 0)^2` for positive `T`; a zero target leaves
   the physical light unchanged. This reaches `T` at zero illumination, keeps
   at least half of physical-light contrast at a fixed local target, and becomes
   the unchanged solve above `4*T` with a continuous first derivative. Neither chart means nor
   chart sizes affect it, so repartitioning a coplanar surface cannot add a
   brightness step. The stored directional moment stays unchanged; the colour
   ratio is inverted to realize the exact lift at the receiver normal. The
   target is attenuated by the receiver's water depth first. Probe irradiance
   uses the same local response. Switchable layers never receive this fill,
   and their emitters are excluded from the permanent target. The prepared
   path does not add the vertex-lit model's global `0.10` ambient floor.

Ray origins use a normal offset of four `f32` machine epsilons scaled by world
coordinate magnitude, plus a tiny chart-interior inset at boundaries. Shading
positions remain shared. The watertight triangle test evaluates projected edge
functions in `f64`; there is no fixed-distance origin dead zone that could
skip a nearby stair riser. Only the far emitter endpoint gets a scale-aware
reconstruction allowance.

Architectural wall slices retain their parent wall's room ownership and
material gradient. The parent room is selected from the complete wall's
contact area; adjoining faces use a real height-aware room volume, so a tall
lintel cannot inherit a shorter corridor merely through its doorway. The
`.92` bottom to `1.05` top material shade is evaluated in the parent wall's
height frame, including the local gable profile, rather than restarting on
each opening fragment. Ceiling-bounded wall faces and their end caps evaluate
that roof on each thickness edge; a sloping roof therefore meets the inner
face without a half-thickness gap. Explicitly authored rigid heights retain
their meaning. Ceiling chart domains exclude only proven wall-covered areas;
all remaining cells share one exposed-region label, allowing the existing
planar merger to join them across doorway subtraction cuts. Invisible wall
texels cannot darken the ceiling edge, and doorway cuts do not create
unnecessary chart boundaries across the room.

Probes for the `off` variant and the fallback remain the historical
display-space model; see §7.3.

### 7.1.2 Irradiance field for moving objects

Compiled variants carry a prepared `blobs/<sha>.irradiance` field (a uniform 3D
grid over the mapped world). Every probe stores the same irradiance +
direction-moment pair as a lightmap texel plus the room it occupies; the
compiler solves it from the same transport pass as the atlas. Moving objects
and characters read it at runtime
with a trilinear interpolation restricted to the sample's own room, so light
does not bleed through floors, ceilings or full-height walls; an unresolvable
position falls back to the vertex-lit sample instead of going black. The GPU
receives the sampled display value through the per-object `light_scale`
uniform — no per-frame bake and no ray is cast at runtime.

### 7.1.3 Changeable lights

A `switchable` ceiling fixture's contribution is solved into its **own layer
group** in the HDR atlas. The environment uniform carries the group count and a
live mask; toggling a switch updates the mask and the fixture's emissive face
material, and the shader immediately sums a different prepared set. There is no
runtime chart re-fill, no per-switch bake and no combinatorial scenario atlas.
A level may prepare at most four switchable fixtures; the illumination change
is a real transport change, not an emissive-only tint.

The moving-object irradiance field (§7.1.2) is solved from the non-switchable
transport only: a switchable fixture contributes nothing to that field in any
state, so a moving object or character is never lit by a switchable fixture
even while it is on and its static surfaces are lit. Toggling updates the
static atlas layers, not the field. Per-state field layers are deliberately not
prepared; until they are, treat a switchable fixture as a static-surface light
where moving-object lighting is concerned.

### 7.2 Light sources

A level authors lights in two places; both become the same neutral `LightSource`:

- `ceiling_lights[]` — a fixture placement (`fixture` catalog id, `x`/`z`, `rotation_degrees`, `brightness`/`intensity`, `color`, `mount`, `y`, `range`, `falloff`, `enabled`, `emission`). The fixture family (`FixtureKind`) decides only the emitting rectangle and the visible mesh: fluorescent panel 0.6 × 0.3 m, round downlight 0.22 × 0.22 m, wall sconce 0.20 × 0.09 m, flush mount 0.16 × 0.16 m.
- `props[].lights[]` — up to eight per prop, in the prop's local frame, with an explicit `shape` (point, rectangle or line), offset, rotation, colour, intensity, range and falloff.

A `LightSource` is a shape (`Point`, `Rect`, `Line` — a line is a thin rect), a world position, a colour, an intensity clamped to `0..=8`, a range clamped to `0.05..=64` m (default 6 m) and a falloff (`Smooth` — the `(1-t)²(1+2t)` cushion — `Linear` or `Constant`). It is active when enabled, with a positive intensity and a non-black colour. There is no maximum active count because nothing is dynamic: the bake visits them all.

### 7.3 The vertex-lit equation (preserved `off` variant and fallback)

This is the historical display-space model. It is still the exact contract of
the `off` variant and of every build that falls back after a plan/fill failure,
and its unit tests remain the vertex-lit parity gate. The prepared HDR atlas
does **not** use it.

Per sample point and channel:

```text
room baseline   = clamp(0.42 · c + 0.10, 0.10, 0.52)
                  c = n / (1 + n), n = ln(1 + (power / area) · 500)
                  power = Σ intensity · sqrt(3.5 / ceiling_height) · colour
direct pool     = screen(Σ per light, cap 0.45)
                  ceiling fixtures: 0.60 · intensity · height_factor · lateral · incidence · visible
                    lateral = (1 - horizontal/range)², incidence = vertical/distance,
                    skipped when the sample is not below the emitter
                  wall/prop lights: 0.60 · intensity · height_factor · falloff(d/range) · visible
bounce fill     = screen(Σ per light, cap 0.26)
                  0.26 · intensity · height_factor · smooth_falloff(d / (1.5·range)) · visible
opening blend   = 0.5 · smooth_falloff(d / 6) · (neighbour_baseline - own_baseline)
light           = clamp(baseline + direct + fill + blend, 0.10, 1.0)
```

`smooth_falloff(t) = (1-t)²(1+2t)`; `screen` is applied sequentially per
light, per channel: `pool ← pool + (cap_i - pool) · min(c_i, cap_i) / cap_i`,
with each light's channels capped at `cap · colour[channel] / max_colour`, so a
warm fixture keeps its colour to the cap instead of whitening and every channel
depends only on its own contributions.

`AMBIENT_LEVEL = 0.10` is the floor; an unlit room is dark by design. The
baseline is deliberately the smaller *fill*: the visibility-tested direct pool
is the light that shapes a room and the only term a static occluder can remove,
so the fill leaves the highlight headroom to it. The bounce fill is the room's
first reflected light: a recessed panel does not point at its own ceiling, but
the room's bounce does, so ceilings and upper walls receive a broad weak halo
(0.26 cap) instead of the bare ambient. A room split by opaque internal walls
gets one baseline per connected area, and a doorway still blends the two areas
through the aperture. Floor interfaces and ceiling bodies isolate storeys, so a
fixture cannot light through a slab. Pool visibility is a binary (vertex-lit)
or multi-tap soft (atlas) test against the same wall solids, floor interfaces,
ceiling bodies and prop-derived boxes the geometry and collision use: High
bakes with two taps per axis and 0.075 m prop-occlusion cells, Medium with two
taps and 0.11 m, Low with one tap and 0.15 m. Openings transmit light through
the hole they cut (the wall solid is removed); glass and grille panes do not
have their own material alpha consulted, so a translucent pane transmits
exactly like the opening. A light's query site is prefilted to
`range + hypot(half_w, half_d)` so a solid just beyond the pool's lateral reach
still blocks a far emitter tap.

### 7.4 Atlas and vertex-lit storage

- **Atlas** (the default; lightmaps on): architectural vertex colours are the material factor `tint × directional face shade` with no baked light, the atlas carries the per-texel light, and `light` is the atlas sample.
- **Vertex-lit** (`PLACES_NO_LIGHTMAPS=1`, or the atlas fallback): the bake is folded into each vertex colour by `shade(base, light) = clamp(base × light)`, every vertex carries `LIGHTMAP_NONE`, and `light` is exactly `vec3(1.0)`.

The bake is the same CPU code in both modes; only the storage differs, and `LightmapMode::Off` always bakes with `BakeConfig::HARD` (one visibility tap, 0.15 m prop-occlusion cell) whatever the quality level, which keeps the vertex-lit fallback identical in shape and light to the always-supported path.

The prepared atlas is **one `texture_2d_array` of up to eight 1024² pages**
(eight 512² pages at the Low profile); the vertex's `lightmap_page` byte is the
base page index, so the same shader expression addresses any page count
without a per-page branch. Every page contributes two layers: a linear HDR
`Rgba16Float` irradiance plane and a direction-moment plane holding the signed
vector sum of the per-channel moment vectors; both planes' alpha channels are
reserved (`VK_FORMAT_R16G16B16A16_SFLOAT` in the package).
Each switchable fixture's prepared contribution adds one more pair per page
after the base group; the environment uniform's page count and mask select what
is summed. `WorldVertex` carries `lightmap_uv` as `Unorm16x2` and
`lightmap_page` as a plain float. `surface_light()` samples the selected layer
pairs only when `lightmap_enabled * (1 - step(254.5, page)) > 0.5`, reconstructs
the HDR light from the sampled irradiance and direction moment (§7.1), tone maps
it, then multiplies by `light_scale`; `LIGHTMAP_NONE`
keeps the vertex-lit colour exactly. Sheen, reflection and emission are all
scaled by that same light factor, so a dark room darkens them.

Charts are planned inline while the mesh is emitted with the deterministic
best-short-side-fit MaxRects allocator; a plan or fill failure keeps the
historical vertex-lit mesh, and a level that genuinely needs more than eight
pages reports the named `PageOverflow` failure — a partial or black atlas is
never drawn. The compiler content key follows the effective configuration,
never the overall quality label, so `Low + Lightmaps Full` reuses the same Full
entry as `High + Lightmaps Full` while Medium and Full never collide; it folds
in the occluder fingerprint, a fingerprint of the vertex-lit model's constants
(`model_fingerprint`) and the transport solver's revision, so recalibrating
either model or upgrading the solver invalidates prepared data even when the
level and configuration are unchanged.

Quality is a compile-time choice: **Medium** runs one diffuse bounce with two
emitter taps per axis at 12 texels/m; **Full** runs two bounces with three taps
at 16 texels/m. `off` ships no atlas and the historical vertex-lit mesh. There
is no runtime atlas re-fill and no runtime lightmap disk store; the compiler
reuses whole packages by fingerprint instead. `SOLVER_REVISION` (currently 4:
water/translucent transmission and the authored fill floor) is folded into
`solver_fingerprint`, so a solver change reports every prepared package stale
even though the stored record format is unchanged.

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

There is no light direction to place a real highlight; both lobes are scaled by the `light` factor and neither invents a source. `roughness` is `1 - shine` (or a per-surface shine override), the normal decode
is §6.4, and the master gate is the material's response bit — cleared for a
material with neither normal map nor sheen, and for the whole scene under Low
(Medium and High draw it).

### 7.6 Shadows

There is no GPU shadow system: no shadow render target, no shadow camera or
projection matrix, no depth texture sampled as data, no comparison sampler, no
PCF kernel, no caster list and no per-light shadow pass. In the **prepared
path** every shadow is a direct shadow ray (per emitter tap) or a bounce ray
that fails to reach the cache, both cast offline against the static triangles:
walls block with every opening kind (door, window, vent) cutting the hole it
really cuts; floors and ceilings are real surfaces, so light cannot cross a
storey; a static prop's triangles occlude; fixture housings deliberately do
not, so an emitter never shadows itself. Movable entities are absent from the
transport scene, so they leave no silhouette.

In the preserved **vertex-lit model** every shadow is a surface losing a baked
pool, tested against the box solids: alpha and blend state are never consulted,
so a cut-out grille pane and a translucent glass pane transmit light exactly
like the opening they fill; trim (baseboards, thresholds) does not block, while
structural pieces (half walls, columns, archways, guardrails, stairs) do; the
trim exemption is geometric, not an oversight: a 12 mm threshold step shadows
only the floor it covers, and a 9 cm baseboard hugs its wall so every floor
point is already on the fixture's side of it — a thin occluder box would change
nothing outside the board's own footprint, which the visibility tests pin
(`a_threshold_height_step_shadows_only_its_own_footprint`,
`a_baseboard_against_its_wall_does_not_shadow_the_open_floor`).

### 7.7 Resources

No lighting resource is created per frame. The only new per-frame upload is the
camera uniform (matrix **and** eye), written with `Queue::write_buffer` and
skipped entirely while both are unchanged. No light buffer, light array, shadow
target, shadow sampler or lighting bind group exists. The prepared lightmap is
one level-scoped `texture_2d_array` (`Rgba16Float`; two layers per page, plus
one pair per switchable fixture) uploaded once at install and dropped with the
level. The prepared probe field is CPU memory sampled by the dynamic-object
update; the reflection probes are at most two cubemaps (6 faces of 64²/48² raw
RGBA8 with their offline prefiltered mip chains) uploaded from the package. For
a vertex-lit build the bake is `BakeConfig::HARD` at every level, so geometry
and light are identical and only the response gate differs; for an atlas build
the variant selects density, page size, tap and bounce budgets at compile time
(§7.4).

## 8. Reflections

Reflections are opt-in per material and weighted by the sheen the material already authors, so a rough or dull surface suppresses its reflection instead of mirroring. They are a player setting (Settings → Graphics → Reflections, plus the `PLACES_NO_REFLECTIONS=1` startup override); turning them off removes the planar pass, the probe bake and the reflection texture binds from the frame without a level reload.

**Probes.** Captured **offline** by `places-compile` with the same scene body
the player installs (static, props, dynamics, decals) and **prefiltered offline
into a roughness mip chain** by `src/render/common/probe_filter.rs`: level 0 is
the capture, level `L` is a cone average whose roughness is
`L / (levels - 1)` (48-texel faces carry 6 levels, 64-texel faces 7). The
package carries the whole chain; the player uploads it and the shader selects
`roughness * probe_max_mip`, so a polished floor reads a nearly sharp capture
and a rough one a wide prefiltered lobe. Live captures (a developer probe
re-bake) keep one valid level and use the bounded two-tap fallback. The nearest
probe to the visible reflective surface is selected per frame, and a black cube
is the fallback when no probe exists. Two conventions are pinned. Face row
order: a cube face captured into a render target has its first row at the top,
while conventional cube-map sampling expects the captured image bottom-up, so
the probe projection negates NDC `y` (storing the bottom-up image) and the
capture pipeline uses the reversed front face to compensate for the winding
flip. Bake position: the routing's centroid is lifted `+1.2 m`
(`PROBE_LIFT_M`). Both are verified by
`render::wgpu::reflections::tests::the_cube_round_trip_matches_the_reference_face_convention`,
a GPU round-trip test (ignored by default) that captures a world-space quad
with the real capture matrices and samples it back, checking layer selection,
the `s` axis and the `t` axis. The mip chain never regenerates at load, on a
graphics change or per frame.

**Planar mirror.** One plane per frame (the nearest whose reflective bounds survive the cull; Medium and High only), half the render size, its own depth, cleared to the raw clear colour. The capture skips the active plane's own static batches: the routing records the plane per mesh range, the material identity includes that plane, and each draw's material state is excluded exactly when its own plane is the capture plane. A material used on two disjoint floor patches therefore keeps two routes — selecting one reflects and excludes its ranges without treating the other plane's ranges as part of it. Sampling uses the projected `uv`, with the `v` flipped because the capture target's first row is NDC `+y`.

## 9. Props, dynamics, doors, fixtures and emission

**Props / GLB models.** The neutral build's `PropMeshBatch` list is uploaded verbatim: world-space, per-vertex-lit vertices, one draw per primitive, model sheets through the texture cache with clamp wrap and the player's filter. Materials are plain-opaque with the primitive's emission (and its mask); props never use normal maps, alpha modes or reflections. Under every quality variant the prop path keeps this CPU vertex-lit bake (`LIGHTMAP_NONE`), not the HDR atlas: the light is position- and room-dependent through the vertex-lit model, but it does not carry the transport solver's HDR, multibounce or dominant-direction response the static surfaces receive.

**Dynamic objects.** One small model-space buffer per model, one group-3 environment per object carrying its model matrix and its baked-light probe (`light_scale`), refreshed only when an object moves. Opaque dynamic primitives splice in after the static opaque class; a dynamic submesh may instead declare a **blended** alpha contract, in which case it draws after the sorted static translucent surfaces, with depth writes off and depth testing against the opaque pass, so a moving glass pane composites over the static world. Dynamic objects are outside the static batches and the bake and cast no shadow. The engine's washer-drum demonstration is spawned by `set_dynamic_demo` for levels that ship one.

**Doors.** A level's `doors[]` frame and leaf are built in code (`src/render/common/doors.rs`) as ordinary `PropModel`s on the dynamic path and synced from `door::Doors` every frame: the leaf's transform and its `DoorCollider` are derived from the same runtime angle, so the drawn slab and the physical stop can never disagree. The interior leaf is a white painted slab with two raised panels per face and a round brass handle on both sides; the sauna leaf is cedar stiles and rails around a clear glass panel. The sauna panel is the blended dynamic submesh: it is drawn after the sorted static translucent surfaces, exactly like any other dynamic blend. A door's textures resolve through the level's `MaterialTable` from its authored material ids or its kind defaults.

**Effects (steam).** An `effects[]` entry contributes no collision, no occlusion and no bake term; it is a bounded plume of blended billboards. The neutral `EffectScene` (`src/render/common/effects.rs`) resolves one scene per level from the level's `MaterialTable` (the same material resolution a door uses) and evaluates every particle's position, size and alpha as a **pure function of the animation clock** — the scene stores no particle state, so two frames at the same clock produce byte-identical vertices and the motion cannot accumulate at any frame rate. Particles are bounded inside their emitter's own volume (`x`/`z` within half the footprint plus `drift`, `y` between the base and base + `height`).

The backend (`src/render/wgpu/effects.rs`) allocates **one vertex/index buffer pair sized once to the level schema's worst case** (64 emitters × 128 particles = 8192 billboards, 32768 vertices, 49152 indices), writes only the used vertex range per frame, and draws one contiguous range per distinct effect material in deterministic emitter order. The pass is straight-alpha (`SrcAlpha`/`OneMinusSrcAlpha`) with `LessEqual` depth testing and **depth writes off**, no culling; it runs after the decals over the finished world body, depth-testing against it and not ordered against the world's own translucent draws. Billboards are **not** sorted back to front (a soft plume a few tens of centimetres deep cannot show the order), are **not** captured into the reflection probes or the planar mirror, and never contribute to baked light. A level with no effects costs nothing; the load-time line reports emitters, particles, draws and sheets.

**Water surfaces.** A level's `water[]` volumes contribute one quad each at their `surface_y` into the ordinary static mesh's floor family (`src/render/common/water.rs`): the material's `alpha_mode: "blend"` contract puts them in the sorted back-to-front translucent pass with depth writes off and no culling, so the same quad is the surface seen from above and from below the waterline. The vertex colour carries the baked light of the corners, the vertex alpha carries the volume's authored `opacity` while the catalog material stays opaque, and the quad is never lightmapped — it stays out of the atlas and is lit by per-corner sampling, exactly like a fixture face or a glass pane. Nothing else is emitted: the basin floor and walls are the level's own room and floor-region geometry.

**Characters.** A placed prop whose model carries a glTF skin is claimed by the character path instead of the static prop draw (`src/render/common/character.rs`): the bind pose is still baked into the ordinary prop batch (light occlusion and the shipped-asset checks are untouched) and only that model's GPU prop draws are suppressed. The neutral animator keeps one blend weight per locomotion state — the current state approaches one exponentially with a 0.18 s time constant, walking advances a gait phase per metre travelled and swimming at a fixed 1.1 Hz — and produces one model-space skinning delta per joint. The backend (`src/render/wgpu/character.rs`) re-skins a character's vertices on the CPU into its own `VERTEX | COPY_DST` buffer **only on the frames its pose revision changes**, draws one indexed draw per primitive after the dynamics with frustum culling, and carries the placement through a per-character group-3 environment. A rig with clips plays the clip its name maps to (`idle`, `walk`/`run`, `jump`/`air`, `swim`) and crossfades over the same time constant; a rig with no clips uses the procedural gait (classified leg pairs, tail chain and body chain). Baked light is sampled once per vertex at spawn, so a character is lit like a static prop and moves without a re-bake.

**Entity cues, live transforms and routes.** A map-authored route or a
`play_animation` action addresses a character by its placed-instance id
(`entity::EntityFrame`). `CharacterScene::update(delta, locomotion, frames)`
matches frames to characters and then calls `CharacterAnimator::update_cued`
instead of the locomotion driver: the cue (`Idle`, `Walk { speed_mps }` or
`Clip { name, once, paused }`) crossfades from whatever pose is on screen, a
one-shot holds its last key, and a walk/run cue plays the named clip at
`speed / reference_speed_mps` from `asset.extras.places_entity_clips` (falling
back to `WALK_REFERENCE_SPEED_MPS` / `RUN_REFERENCE_SPEED_MPS`), preferring
`run` above 1.5× the walk reference. A frame may also carry a live
`(position, yaw)`: `Character::set_pose` rebuilds the placement matrix,
recomputes the conservative culling bounds, and `WgpuCharacters::sync` rewrites
that character's group-3 environment matrix and bounds while only re-skinning
the vertices whose pose revision changed. Characters with no frame keep
following the player's locomotion snapshot, exactly as before.

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

**Bloom and resolve.** The scene is drawn into an offscreen colour+depth target and resolved into the display image by one fullscreen pass. Bloom is drawn from the world's **emissive term alone** — never from brightness — so a brightly lit wall cannot glow. The emissive pass shares the scene's depth (an emissive draw that did not survive produces no pass), followed by a quarter-size two-pass 5-tap blur. The resolve adds bloom, exposure, a tone shoulder that leaves everything below 0.75 untouched, and a subtle grade; it is the only place a scene pixel becomes a display pixel. Bloom is a **player setting** (Settings → Graphics → Bloom, plus the `PLACES_NO_BLOOM=1` startup override), not part of the quality level, so `High + Bloom Off` and `Low + Bloom On` are both valid. With Bloom off no emissive or blur pass is submitted and the bloom targets are left allocated but unused; with `Low` plus Bloom off the resolve stage is the identity and presents the scene with the plain copy quad. The resolve and HUD run at the drawable's resolution at every level; the scene target is the level-sized one (§12). The emissive and blur targets remain raw display space.

**HUD.** The renderer-owned UI pass (`ui.wgsl`) draws into the raw presented target over the resolved image, at the drawable's resolution, with depth testing off and straight-alpha blending, so semi-transparent panels blend in display space. The layout is authored against a 480×272 reference canvas and scaled by `UiViewport`; the presented target is copied to the sRGB surface with one encode afterwards.

## 12. Quality levels and Texture Filtering

Three runtime quality levels — **Low**, **Medium** and **High** (the default) — use the same assets, ids and level content. A level is selectable while playing; a change is applied as one diffed transaction (see §12.2): the level's CPU build is retained, only the quality-budget-dependent GPU resources are re-fitted or rebuilt, and an uncached lightmap atlas fills on a worker while the previous lighting keeps rendering. The player, camera and pause state are never touched.

The level is also the overall preset for the three **Advanced** graphics settings. An active quality change cascades them to the preset defaults, and the player may then override each one independently; an override never changes the level's label, and there is no "Custom" level. The Graphics page keeps Bloom and VSync as ordinary top-level rows and hides the three advanced rows behind a collapsible **Advanced** group (Enter toggles it, left/right is ignored, and every newly opened settings screen starts collapsed):

| Overall Quality | Texture Filtering | Lightmaps | Reflections |
|---|---|---|---|
| Low | Low | Off | Off |
| Medium | Medium | Medium | Medium |
| High (default) | High | Full | Full |

The lightmap and reflection budgets are owned by the advanced settings, not the level: the table below is what the preset defaults resolve to, and Lightmaps Off / Reflections Off remove the atlas / probe and planar work entirely whatever the overall level says.

| Feature | Low | Medium | High | Source |
|---|---|---|---|---|
| Surface / fixture / decal sheets | 256 | 512 | 1024 | `QualityLevel::budget` |
| Emission masks | 128 | 256 | 512 | `QualityLevel::budget` |
| Prop sheets | 128 | 256 | 256 | `QualityLevel::budget` |
| Atlas: texels/m, page edge, pages, padding | 9, 512, 4, 1 | 12, 1024, 4, 2 | 16, 1024, 4, 2 | `LightmapQuality::lightmap_config` |
| Bake taps per axis / prop-occlusion cell | 1 / 0.15 m | 2 / 0.11 m | 2 / 0.075 m | `LightmapQuality::bake_config` |
| Surface response (normal map + sheen + reflection strength) | gated off | drawn | drawn | `draws_surface_response` |
| Scene resolution | ≤ 480 wide, aspect preserved | half the drawable, aspect preserved | drawable | `scene_target_size` |
| Presented/resolve resolution | drawable | drawable | drawable | `target_sizes` |
| Planar reflection | off | on | on | `ReflectionQuality::draws_planar` |
| Probe face edge | none | 48 | 64 | `probe_face_size` |
| Post tone knee / grade | 1.0 / none | 0.75 / none | 0.75 / 1.03, 1.02 | `PostSettings::for_level` |
| Fog, emission and its animation, decals, effects, UI | identical | identical | identical | shared code paths |

The lightmap and bake configuration columns are the resolved preset defaults; with the Advanced settings in play the actual bake follows `LightmapQuality`, not the level. `LightmapQuality::lightmap_config`/`bake_config` carry the density, page budget, tap count and prop-occlusion cell, and `LightmapBuildOptions::for_lightmaps` is the renderer's entry point, so `Low + Lightmaps Full` bakes a Full atlas and `High + Lightmaps Off` stays vertex-lit. The content key hashes the concrete configuration and the `Full` profile name, so Medium and Full never share an atlas entry while a Full atlas baked for any overall level does.

### 12.2 Live graphics reconfiguration

Every graphics setter is a cheap recording: `set_quality`, `set_lightmap_quality`, `set_reflection_quality`, `set_bloom_enabled` and `set_texture_filtering` write the requested value and nothing else. The event loop diffs requested and applied settings. `apply_frame_graphics` handles frame-only changes; changes requiring a prepared world use the loading worker and staged installation:

| Change | Work |
|---|---|
| Texture Filtering only | none: the recorded preset selects which bind group a draw binds at bind time |
| Bloom only | none: the value gates the per-frame post settings |
| Reflections only | retire/create probe cubemaps and the planar target, upload the package's captures for the new face size, rebuild the environment bind groups |
| Lightmaps only | worker decoded-variant lookup; on a miss decode the package's variant records (geometry, lighting, atlas, collision, probes), then stage GPU installation |
| Quality only (lightmap configuration unchanged) | reuse a matching immutable prepared build when retained; fit/upload resources for the new GPU quality budget |
| Combined | one latest request and one completed installation |

Startup, level changes and CPU-dependent graphics changes use `loading::Loader`.
Its single worker decodes compiled map packages: it resolves texture pixels
through the installed asset bundle and decodes the package's prepared records
(geometry, props, baked lighting, lightmap atlas, collision, probe captures) for
the requested lightmap quality. It never bakes light, plans charts, fills an
atlas, emits geometry or captures probes; the main thread continues polling
events and presenting the loading UI or previous world. Generation checks reject
superseded results, and cancellation is cooperative between decode stages.

The renderer retains an `Arc<LevelBuild>`; the worker also retains an LRU of up
to three decoded variants within a 192 MiB retained-data budget. Keys are the
package's content identity and the effective Lightmaps quality. Oversized
variants remain usable but are not retained in that cache. A cache hit reuses
geometry, lighting, atlas, compiled collision and probe captures; character
playback state is still built for the request. File-backed texture revisions use
content keys so changed pixels cannot reuse an older GPU upload.

GPU ownership remains on the main thread. `install_prepared` starts a pending
installation and `advance_prepared_install` advances its upload phases; prop
batches are processed with a cooperative time budget. Only a completed
installation replaces the active world. Cancellation discards pending resources.
Individual atlas uploads, material preparation, character uploads and packaged
probe uploads can still take longer than a frame; this lifecycle does not
promise a hard latency bound. Measurements belong in dated reports, not this
execution contract.

Downscaling is a load-time step (`fit_image` → `downscaled_to`) cached with the texture it produced, never a per-frame cost. Low leaves the optional surface response out and renders the 3D scene no wider than the historical 480 px reference width; Medium draws the response and renders at half the drawable; High keeps the native artwork and the drawable-sized scene: the same level, the same materials and the same ids. One deliberate resource difference: because the response is gated off before resolution, the renderer does not upload a normal-map texture at all on Low, while the rendered policy (geometric normal, no sheen) is identical; a live Low→High switch releases the level-fitted textures and re-resolves, so the map appears.

### 12.1 Texture Filtering (player option)

Texture Filtering is one of the three Advanced settings. The overall level cascades its preset (Low→Low, Medium→Medium, High→High) and an explicit choice is an independent override: any option combines with any level, and it changes no pixel data and no GPU resource. It selects which of the three shared world sampler presets a draw binds at bind time (§5.3).

| Player option | Internal world filtering | Mipmaps |
|---|---|---|
| Low | trilinear (linear mag/min/mip) + ~4x anisotropic | full generated chain |
| Medium | trilinear + ~8x anisotropic | full generated chain |
| High (default) | trilinear + ~16x anisotropic | full generated chain |

A missing `"texture_filtering"` key derives its preset from the saved
`"quality"` level, and a value that is present is never overwritten on load:
only an active quality change cascades it. An unknown value falls back to High.

- **Mipmaps are required and always present** for ordinary world sheets: every option filters `Linear`/`Linear`/`Linear` and relies on the generated chain (§5.3). No option disables mips or falls back to point sampling.
- **Hardware fallback.** Anisotropy above 1x requires an adapter with `DownlevelFlags::ANISOTROPIC_FILTERING`. Without it the three options keep the same trilinear filtering and the anisotropy request is clamped to 1x; no device feature is requested and no option becomes unavailable. The `PLACES_VERBOSE` startup line reports the capability and the requested degrees.
- **Texture classes that follow it:** tiling surface sheets (base colour and material normal maps), external decal sheets, the generated decal atlas, fixture faces, prop/entity sheets and emissive masks.
- **Texture classes that do not:** the white fallback sheet and the HUD font atlas stay clamped nearest; the lightmap atlas pages keep their fixed clamped linear sampler and are **independent of the player setting**; probe cubemaps and the planar reflection target stay clamped linear.
- Switching is live: `WgpuRenderer::set_texture_filtering` only records the option and each draw binds the matching bind group. No texture is re-uploaded and the environment binding is not rebuilt, because the lightmap sampler no longer follows the setting.

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
[wgpu] effects: E emitter(s), P particle(s), D draw(s), T sheet(s) (U uploaded, C cached)
[wgpu] post targets: scene 480x270 ... presented 1280x720 ...
```

Recoverable events (surface timeout, surface lost/outdated) are reported at most once per process; a VSync change logs the presentation mode it selected; minimize/restore is silent; there are no per-frame logs.

`BENCH_SUMMARY` reports the last frame the renderer actually submitted: draw calls, total batches, visible and total vertices, texture binds and material changes. A frame that is never submitted (`PLACES_BENCH_NORENDER`, or a surface acquisition that keeps returning `Skip`, e.g. while the window is occluded) reports no draw counts rather than a fabricated number. Count accounting distinguishes per-material bind changes from per-draw material records; draw calls and visible vertices are the comparable numbers.

| Switch | Effect |
|---|---|
| `PLACES_LEVEL`, `PLACES_SPAWN`, `PLACES_CAMERA` | level, spawn and camera selection for a run |
| `PLACES_QUALITY=low\|medium\|high` | quality level for one run (a session override: it does not cascade the advanced settings) |
| `PLACES_NO_LIGHTMAPS=1\|off\|medium\|full` | pin the Lightmaps quality for one run (`1` forces Off) |
| `PLACES_NO_REFLECTIONS=1\|off\|medium\|full` | pin the Reflections quality for one run (`1` forces Off) |
| `PLACES_NO_BLOOM=1` | disable the bloom stage for one run |
| `PLACES_CAPTURE`, `PLACES_CAPTURE_FRAME` | one-frame PNG capture path |
| `PLACES_SCREEN=settings\|graphics\|advanced` | open a screen on the first frame for a capture (`advanced` expands the Graphics page's Advanced group) |
| `PLACES_VERBOSE=1` | developer telemetry (including the summarized `[startup]` phase table after the first present) |
| `PLACES_BENCH=1`, `PLACES_BENCH_FRAMES`, `PLACES_BENCH_WARMUP`, `PLACES_BENCH_OUT` | benchmark harness |
| `PLACES_BENCH_NOSWAP`, `PLACES_BENCH_NORENDER`, `PLACES_BENCH_FINISH`, `PLACES_BENCH_NOCULL` | submission diagnostics |
| `PLACES_BENCH_WINDOW_CYCLE=<frame>:resize:<w>x<h>\|minimize\|restore[,...]` | scripted live window lifecycle through the real SDL window |
| `PLACES_BENCH_QUALITY_CYCLE=<frame>:<level>[,...]` | scripted live quality switches through the normal rebuild path (`low`/`medium`/`high`); cascades the advanced presets |
| `PLACES_BENCH_GRAPHICS_CYCLE=<frame>:<setting>=<value>[,...]` | scripted live Advanced changes through the normal settings setters (`filtering=low\|medium\|high`, `lightmaps=off\|medium\|full`, `reflections=off\|medium\|full`, `bloom=on\|off`) |

## 14. Provenance and recorded parity baselines

The renderer was ported feature-for-feature from the project's former GLES2 implementation, which is preserved in Git at the `renderer-gles2-reference` tag (commit `797370e17aab1409a5de3ea70b9a68f742452`); mainline has no dependency on it, and the historical renderer audit and parity evidence live in that snapshot and in mainline Git history. See [RENDERER_REFERENCE.md](RENDERER_REFERENCE.md) for the map.

The recorded comparison figures below are the current renderer measured against that preserved implementation on macOS/Metal (same asset root, 1280×720, default settings, per-pixel worst RGB channel, 0..255). They bound future regressions; a capture compared between two builds of this renderer can require byte equality instead. **The lighting rework (four-page array atlas, directional pools and bounce fill) intentionally changes lit pixels, so the atlas/lightmap-related rows below are the pre-rework records and no longer describe the current bake; they remain the preserved implementation's comparison basis.**

| Metric | Value |
|---|---|
| Canonical 50-view set (25 views × High/Low): smallest per-view mean | 0.103 (`low/drum`) |
| Canonical set: largest per-view mean | 0.854 (`high/pool_entry`) |
| Canonical set: mean of the 50 view means | 0.419 |
| Canonical set: largest share of pixels over 8 | 0.213 % (`high/office`) |
| Canonical set: largest single channel difference | 163 (`high/pool_wide`) |
| Views improved by more than 0.02 mean | 39/50 |
| Views regressed by more than 0.02 mean | 0/50 |
| Expanded campaign (40 views × High/Low) | 80/80 captured; per-view means 0.000–0.973; hottest 80×45 block 0–3/255 |
| Lightmap atlas pages | byte-identical between the two renderers, High and Low |
| A/B contribution correlation: planar / lightmap / probe | 0.9994 / 0.991–0.993 / 0.993–0.994 |
| A/B contribution magnitudes (mean, max): planar / lightmap / probe / bloom | 3.931 vs 3.922, 32/32 · 5.054 vs 5.040, 49/49 · 1.062 vs 1.057, 5/5 · 0.067 vs 0.063 |
| Places Demo draw set (canonical frame, recorded before the character path) | 118 static + 37 prop + 1 dynamic + 3 decal draws, plus the UI pass; static upload 6,411 vertices / 10,470 indices / 118 draws. The demo's one skinned placement adds three character draws and suppresses those three static prop draws |
| Lifecycle: texture residency High / Low | 188,743,640 B / 15,728,600 B, stable across the scripted High↔Low rebuilds |

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
- **Lighting:** the sheen equation (CPU mirror), the display-space assembly order, the unlit bypass conditions, the vertex-lit build's byte-for-byte mesh, the lightmap CPU mirror of `surface_light`, the `needs_upload_fallback` rule. The rework adds: the directional ceiling pool's row profile (brightest beneath the fixture), the scalar-per-channel screen with colour-scaled caps, bounce-fill energy, the query-site radius covering the emitter extent, zone-seam continuity on a narrow strip, the bounded MaxRects allocator and its over-budget rejection, The Pit baking within the four-page budget at both profiles, a distant room leaving a lit room's light unchanged, light-order stability, baseboard/threshold non-participation.
- **Reflections:** the six face directions/ups, the 90° projection with the Y flip, the planar mirror composition, `+1.2 m` bake position, nearest probe/plane rules, and the ignored GPU cube round-trip.
- **Characters:** the skinning delta maths against a synthetic two-bone rig, bind-pose bounds, joint/weight parsing and malformed-skin rejection, blend-weight convergence, distance-driven walking phase, frame-rate-independent playback at 30/60/144 fps, clip name mapping and LINEAR/STEP sampling, crossfades, orthonormal finite matrices across state switches, and no per-frame reallocation.
- **Water:** one translucent floor quad per authored volume at its surface height, the volume's opacity in the vertex alpha, no lightmap page, the `blend` material in the sorted translucent pass, and back-to-front ordering shared with the other translucent surfaces.
- **Doors:** the interior and sauna model builds (panel counts, static frame versus moving leaf, the sauna glass submesh's blended contract), the closed/opening/open/closing phase machine, the angle-to-collider maths, `stop`/`reverse` obstruction handling and reset-to-authored-state, and dynamic draw order (a blended dynamic submesh after the sorted static translucent surfaces).
- **Effects:** particle poses as a pure function of the clock (byte-identical vertices at the same clock), bounds inside the emitter volume, the exact per-level particle/vertex/index budgets, deterministic material grouping and one draw per distinct effect material, the default steam material resolving through the table, and the blended depth-testing/no-depth-write pass state.
- **Post/UI:** blur kernel and step, target sizes (scene = level, presented = drawable), resolve maths, the single-conversion contract, ortho corners, viewport maths, blend factors.
- **Integration:** the world pipeline variants and their states, emissive flag propagation, material reflection-mode rules, live window and quality cycle parsing (`PLACES_BENCH_WINDOW_CYCLE`, `PLACES_BENCH_QUALITY_CYCLE`, `PLACES_BENCH_GRAPHICS_CYCLE`).

Five diagnostics are intentionally ignored by default: two GPU measurements (requiring an adapter) and three developer measurement/reporting tools. They run explicitly with `cargo test --all-features --bin places -- --ignored`; see [VERIFICATION.md](VERIFICATION.md).
