# wgpu bootstrap (Stage 4)

Status: **the Stage 4 lifecycle is implemented in the working tree behind the
Stage 3 renderer boundary.** It owns the SDL surface, device and presentation
lifecycle. Stage 5 added the world geometry on top of it (see
[WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md)); the Stage 4 sections below
that describe a clear-only frame are the Stage 4 record, and the places Stage 5
changed them are marked. **Stage 10 note:** the backend now draws the complete
reference frame (Stages 8-9) and its lifecycle — resize, minimize, reload,
quality switch, capture, shutdown — was validated in Stage 10
([WGPU_STAGE10.md](WGPU_STAGE10.md)). The OpenGL renderer remains the
reference implementation and the default.

This document is the Stage 4 record: it says what Stage 4 established, why, and
what later stages may rely on.

## 1. wgpu version and dependencies

| Item | Choice | Why |
|---|---|---|
| wgpu | `30.0.1` | Latest release at implementation time; needs Rust 1.87, the workspace uses 1.91+. |
| pollster | `1.0` | Blocks on the two wgpu futures (`request_adapter`, `request_device`) without pulling in an async runtime. |
| sdl2 feature | `raw-window-handle` (0.6) | Implements `HasWindowHandle`/`HasDisplayHandle` for `sdl2::video::Window`; wgpu 30 uses raw-window-handle 0.6. |

wgpu is declared with `default-features = false` and one native desktop backend
per target:

```toml
wgpu = { version = "30.0.1", default-features = false, features = ["std"] }

[target.'cfg(target_os = "macos")'.dependencies]
wgpu = { ..., features = ["metal"] }
[target.'cfg(target_os = "linux")'.dependencies]
wgpu = { ..., features = ["vulkan"] }
[target.'cfg(target_os = "windows")'.dependencies]
wgpu = { ..., features = ["dx12"] }
```

The GLES, WebGL and WebGPU backends are never compiled in, so a silent fallback
to the OpenGL API is impossible by construction. `wgsl` was not enabled in
Stage 4 (no shader module existed); **Stage 5 enabled it** for the world
pipeline.

## 2. Temporary renderer selector

`PLACES_RENDERER` chooses the implementation **once, before the SDL window is
created**:

```text
PLACES_RENDERER=opengl   (default)  the complete reference renderer
PLACES_RENDERER=wgpu                the Stage 4 bootstrap renderer
```

- Unset or empty selects `opengl`.
- Any other value fails the process with a message naming the value and the
  accepted choices.
- There is no UI switch, no saved setting, no runtime hot-swap and no automatic
  fallback: `PLACES_RENDERER=wgpu` either initializes wgpu or reports why not.

This is a migration mechanism, not a player-facing feature; it is expected to
be replaced by a permanently selected backend once the wgpu world renderer is
complete (Stage 5+).

## 3. Native backend policy

| Platform | Backend | Verified |
|---|---|---|
| macOS | Metal | runtime (`PLACES_RENDERER=wgpu`, adapter line reports `Metal`) |
| Linux | Vulkan | source/configuration reviewed; Stage 15 validates |
| Windows | Direct3D 12 | source/configuration reviewed; Stage 15 validates |

`render::wgpu::surface::NATIVE_BACKEND` is the single backend per target. Before
`Instance::new`, the renderer checks that the platform has a compiled backend.
After adapter discovery it verifies `AdapterInfo::backend` equals the expected
one; a mismatch is a hard error, never a fallback. The adapter request sets
`force_fallback_adapter: false` (no software adapter as the normal path) and
uses the default power preference.

## 4. SDL2 surface strategy

SDL2 remains the platform layer. The wgpu path builds its window with
`metal_view()` on macOS (SDL requires an `SDL_MetalView` for its
raw-window-handle implementation) and requests **no** OpenGL attributes and no
`.opengl()` flag; no GL context is created. `main` parses the selector before
`SDL_CreateWindow`, so the two paths never share window assumptions.

`render::wgpu::surface::create` copies the window's raw window/display handles
with `SurfaceTargetUnsafe::from_display_and_window` and calls
`Instance::create_surface_unsafe`. Safe surface creation is not usable here:
`Surface<'window>` would borrow the `Window` for the renderer's lifetime while
`main` still needs `&mut Window` for `set_size`/`set_fullscreen`, and routing
all window mutation through the renderer is far larger than this stage
justifies.

The unsafe block is isolated in one function with a written `SAFETY`
argument. The invariant is:

- the renderer is a local in `main` declared **after** the SDL window, so Rust
  drops it before the window on every path, including error returns;
- SDL's video subsystem and the window (and its `SDL_MetalView`) live for the
  whole frame loop, which is the only time the surface is used;
- the renderer never touches the window in `Drop`.

No `transmute`, no faked `'static` in the type system (the surface type is
`Surface<'static>` by the raw-handle API's contract, upheld by the invariant
above), no leaked window, and no reliance on undocumented destruction order.

## 5. Initialization order

```text
parse PLACES_RENDERER
        ↓
initialize SDL + video subsystem
        ↓
create the window for the chosen backend (GL flags vs metal view)
        ↓
OpenGL:  create GL context, build programs/textures          (unchanged)
wgpu:    create Instance (native backend mask)
             ↓
         create Surface from the SDL window
             ↓
         request compatible hardware Adapter
             ↓
         verify AdapterInfo::backend == native backend
             ↓
         query SurfaceCapabilities
             ↓
         request Device + Queue
             ↓
         install the device-lost callback
             ↓
         select format/present mode/alpha mode
             ↓
         configure on the first frame with a nonzero drawable
             ↓
         create the depth target with the same size
```

Destruction is the exact reverse, enforced by field declaration order inside
`WgpuRenderer`: the pending frame (if any) is discarded first, then the surface,
then the device, queue and instance, with the depth target and the remaining
plain state following. Two ordering constraints matter: the pending frame must
precede the surface because discarding it reaches back into the surface's
swapchain, and the surface must precede the device/instance so the layer it owns
is released before the device it references.

## 6. Surface format policy

Deterministic preference over the capabilities the surface actually reports:

1. `Bgra8UnormSrgb`
2. `Rgba8UnormSrgb`
3. the first format the surface reports

The preferred choices keep the current presentation intent (the OpenGL
framebuffer is sRGB); the third rule means an unusual surface is never blocked
by a hard-coded format. `SurfaceColorSpace::Auto` is used; no Stage 14
gamma-parity work is attempted here.

## 7. Presentation mode policy

- `VSync` on (the shipped default): `Fifo`, then `FifoRelaxed`.
- `VSync` explicitly off: `Immediate`, then `Fifo`.
- Nothing supported: the first mode the surface reports, else `Fifo`.

Stage 4 never enables uncapped/tearing presentation by default, and
`set_swap_interval` reconfigures the surface live when the player changes the
setting, exactly as the OpenGL path reapplies `SDL_GL_SetSwapInterval`.

## 8. Depth target

One main depth texture, `TextureFormat::Depth32Float`
(`render::wgpu::surface::DEPTH_FORMAT`), `RENDER_ATTACHMENT` usage, sized to the
configured surface, cleared to `1.0` in the same pass as the colour clear. It is
recreated only when the drawable size changes. There are no shadow, lightmap or
reflection depth resources; the constant is shared so Stage 5's world depth
testing uses the same format.

## 9. High-DPI and drawable size

The configured size is always `window.drawable_size()` — physical pixels — never
the logical window size. On Retina macOS that is the backing-scale size. `main`
re-queries it every frame and calls `Renderer::set_drawable_size`; the renderer
reconfigures the surface and recreates the depth target only when it changed.

## 10. Resize, minimize and restore

```text
drawable size changed
        ↓
needs_configure = true (single flag)
        ↓
next frame:
  size != 0 → recreate depth if its size changed,
              Surface::configure,
              clear/present normally
  size == 0 → skip acquisition entirely, keep the event loop alive
```

A zero-sized drawable (minimized or hidden window) is not a fatal error and not
a busy loop: `main` already sleeps ~16 ms on those frames, and the renderer
configures nothing until a valid size returns. Restore is automatic — no
application restart, no device recreation.

## 11. Surface-error handling

`Surface::get_current_texture` returns `CurrentSurfaceTexture`; Stage 4 maps it:

| Result | Action |
|---|---|
| `Success`, `Suboptimal` | draw into the acquired texture |
| `Timeout` | skip the frame, one warning per process |
| `Occluded` | skip the frame silently |
| `Outdated` | reconfigure once, retry once (then skip) |
| `Lost` | recreate the surface from the SDL window on the next present, reconfigure, resume |
| `Validation` | fatal: report and stop |

At most one recovery attempt is made per frame; there is no retry loop.
Reconfiguring drops any acquired-but-unpresented frame first, because wgpu
forbids configuring a surface while one of its textures is alive.

## 12. Device-error handling

`Device::set_device_lost_callback` records the reason on a shared slot. The
frame loop checks it before issuing work; the first fatal error is reported once
(`[wgpu] fatal device error: ...`), all later GPU work is skipped, and the
application stops through its normal shutdown path with a non-zero exit.
Validation errors are not suppressed or captured globally: wgpu's default
handler still makes them visible during development.

## 13. Clear/present frame

`Renderer::render_scene(camera)` for the wgpu variant:

1. acquires the current surface texture,
2. creates a texture view,
3. creates a command encoder,
4. begins one render pass with colour and depth attachments,
5. clears colour to a stable dark neutral and depth to `1.0`,
6. **Stage 5:** binds the world pipeline and camera, then draws the uploaded
   static architectural geometry (frustum-culled, indexed, depth-tested,
   back-face culled) — see [WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md),
7. ends the pass, submits the command buffer,
8. keeps the texture; `Renderer::present(window)` presents it (or cycles the
   swapchain when `PLACES_BENCH_NORENDER=1` skipped the scene).

Stage 4 drew no Places content: no shader module, no render pipeline, no bind
group, no vertex/index buffer, no texture upload, no camera uniform, no world
geometry, no lighting/shadow/lightmap/reflection/decal work, and no UI. Stage 5
added the shader module, world pipeline, camera uniform, world buffers and
draws listed above; Stage 6 added the texture cache, mip chains, shared
samplers and base-colour sampling (see [WGPU_TEXTURES.md](WGPU_TEXTURES.md));
Stage 7 added the material system: resolved material states, normal maps, the
opaque/cut-out/translucent pipelines and the material WGSL (see
[WGPU_MATERIALS.md](WGPU_MATERIALS.md)).
**Lighting, shadows, lightmaps, reflections, emission, props, fixtures, decals
and UI are still absent.** `PLACES_CAPTURE` still reports that the wgpu backend
cannot capture frames rather than writing a wrong image.

## 14. Diagnostics

With `PLACES_VERBOSE=1` one startup line names the resolved chain:

```text
[renderer] wgpu | adapter: <name> | backend: Metal | device type: ... |
surface format: ... | present mode: Fifo | alpha mode: Opaque |
depth format: ... | drawable: 1280x720
```

Recoverable events (surface timeout, surface lost/outdated) are reported at most
once per process; a `VSync` change logs the presentation mode it selected;
minimize/restore is silent; there are no per-frame logs. Stage 5 adds two
load-time lines: `[wgpu] world upload: N vertices, M indices, K draws in C
chunk(s)` and `[wgpu] world pipeline for surface format <format>`. Stage 6 adds
the texture line, `[wgpu] textures: N unique, U uploaded, ...`; Stage 7 adds
the material line,
`[wgpu] materials: N resolved (R response, E reflection-eligible), M normal
maps (U uploaded, H cache hits), O opaque / C cutout / T translucent of D
draws (response ...)`. See [WGPU_TEXTURES.md](WGPU_TEXTURES.md) §17 and
[WGPU_MATERIALS.md](WGPU_MATERIALS.md) §21.

## 15. Handoff to Stage 5 (delivered)

Stage 5 added world geometry, vertex/index buffers, camera matrices,
projection, depth testing, face culling and the first WGSL pipeline
**without revisiting**:

- instance/surface/adapter/device/queue creation and teardown;
- backend selection and the no-GL-fallback guarantee;
- SDL window creation for wgpu;
- surface configuration, format/present-mode selection and resize handling;
- the main depth format and its recreation rule;
- fatal device-error propagation and clean shutdown.

Delivered Stage 5 additions: the `wgsl` cargo feature, a shader module, a render
pipeline, a per-frame camera uniform buffer/bind group, vertex and index
buffers filled from `render::common` data, depth testing and culling in the
pipeline state, and the replacement for `Renderer::render_scene`'s ignored
camera. See [WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md).
