# SDL3 migration (Stage 12)

Stage 12 replaced the SDL2 platform layer with SDL3 without changing the game,
the wgpu renderer, the input bindings, the window behaviour or the rendered
output. This document records what changed, why, and how it was verified.
Historical documents (Stage 4-11 records, `RENDERER_AUDIT.md`, the frozen
renderer baseline) keep their SDL2 terminology as history.

## 1. Previous role of SDL2

SDL2 was the desktop platform layer only. It owned the SDL context, the video
subsystem, the window, the event pump and the key events; the renderer was
already wgpu (Metal/Vulkan/D3D12) with the window reaching it through the
raw-window-handle path. There was no GL context, no SDL renderer, and no
SDL_GPU. Timing is `std::time::Instant`; neither SDL2 nor SDL3 plays any part
in frame pacing. The complete pre-migration inventory is in
`target/agent-work/stage12/contracts/sdl2-inventory.md` (132 classified touch
points: 22 surface, 18 keyboard, 12 event, 10 display, 9 benchmark, 8 init,
6 window creation, 6 window flag, 5 fullscreen, 4 DPI, 2 resize, 3 other,
22 test-only, 5 packaging; zero mouse, focus or timing sites).

## 2. Scope

Migrated:

- SDL initialization and the video subsystem;
- window creation, title, size fitting, centering, resizable and HiDPI flags;
- fullscreen enter/leave and fullscreen state polling;
- display lookup (usable bounds, fullscreen bounds);
- the event pump (quit, key down, key up);
- keyboard handling, key repeat, rebinding capture;
- logical window size vs physical pixel size;
- the wgpu raw-window-handle surface and its recreation on loss;
- benchmark window lifecycle calls (`set_size`, `minimize`, `restore`);
- app metadata/hints;
- tests, benchmark tooling and documentation that referenced SDL2.

Not migrated, by design: the wgpu renderer, WGSL, materials, lightmaps,
reflections, props, decals, fog, post-processing, UI design, level format,
assets, GLBs, textures, map content, gameplay, physics, audio, controller
design and the editor. SDL_GPU, SDL_Renderer and SDL canvas rendering are not
used, and the SDL main-callback architecture was not adopted: Places keeps its
ordinary Rust `main()` and frame loop.

## 3. Dependency and linking

```toml
sdl3 = { version = "0.20", features = ["use-pkg-config", "raw-window-handle"] }
```

- `sdl3` 0.20.0 (MIT) with `sdl3-sys` 0.7.1+SDL-3.4.16 (bindings for SDL
  3.4.16, zlib).
- `use-pkg-config` resolves the system SDL3 (`brew install sdl3`; Homebrew
  3.4.16 on this machine) exactly like the previous `sdl2` dependency used
  pkg-config. `sdl3-sys` enables pkg-config probing by default; the feature is
  named explicitly so the intent is visible.
- `raw-window-handle` pulls raw-window-handle 0.6.2 — the same version wgpu
  30.0.1 already uses — and implements `HasWindowHandle`/`HasDisplayHandle`
  for `sdl3::video::Window`.
- The bindings' own floor is SDL 3.1.3, but Places calls `SDL_SyncWindow`
  (SDL 3.2.0), so the practical minimum is **SDL 3.2.0**; this machine runs
  3.4.16, the version the bindings target.
- No `image`, `ttf`, `mixer`, `net`, `gfx`, `main`, `ash`, `static-link`,
  `build-from-source*` or `link-framework` feature is enabled.

The release binary links `/opt/homebrew/opt/sdl3/lib/libSDL3.0.dylib`
(`otool -L`); it no longer links SDL2 in any form. This matches the existing
deployment model (executable + assets; SDL is a system dependency), and it
does not add a runtime dependency: Homebrew's `sdl2` formula is now
`sdl2-compat`, which itself requires `sdl3`, so the pre-migration binary
already loaded SDL3 at runtime.

Falling out of the swap: `sdl2`, `sdl2-sys`, `bitflags 1.3.2`,
`lazy_static` and `version-compare` left `Cargo.lock`; `bitflags` is now a
single 2.x version, so `clippy.toml` no longer allows that duplicate.

## 4. Initialization and app metadata

SDL2 set `SDL_VIDEO_X11_WMCLASS` and `SDL_APP_NAME` hints. SDL3 removed the
window-class hint in favour of app metadata, so `main` now calls

```rust
sdl3::set_app_metadata(
    Some("Places"),
    Some(env!("CARGO_PKG_VERSION")),
    Some("io.github.csd113.places"),
)?;
```

before `sdl3::init()`. The identifier `io.github.csd113.places` is preserved.
Text input stays disabled: SDL3 leaves it off until `SDL_StartTextInput`, and
the explicit `text_input().stop(&window)` from SDL2 is kept after window
creation as defence in depth, so the macOS IME path stays disengaged and key
events remain raw.

## 5. Window and size semantics

| Concept | SDL2 | SDL3 |
| --- | --- | --- |
| logical size | `window.size()` | `window.size()` (window coordinates) |
| physical drawable | `window.drawable_size()` | `window.size_in_pixels()` |
| HiDPI request | `allow_highdpi()` | `high_pixel_density()` |
| borderless fullscreen | `builder.fullscreen_desktop()` | `builder.fullscreen()` (no display mode set = borderless desktop) |
| runtime fullscreen | `set_fullscreen(Desktop/Off)` | `set_fullscreen(true/false)` |
| display identity | `display_index() -> Option<i32>` (0 = primary) | `get_display() -> Result<Display>`; fallback `video.get_primary_display()` |
| bounds | `video.display_bounds(i)` / `display_usable_bounds(i)` | `Display::get_bounds()` / `get_usable_bounds()` |
| window position | `position_centered()` | `position_centered()` (unchanged) |

The renderer still reads only the physical pixel size. On this machine the
canonical 640x360 logical window is 1280x720 pixels at 2x, and the default
1920x1080 request is clamped to the 1512x870 work area and reported by the OS
as 1512x838 logical / 3024x1676 pixels — identical to SDL2.

`FullscreenType::Desktop` is unreachable in SDL3 (the crate's variant tests
the old SDL2 flag bit, which is `SDL_WINDOW_MODAL` in SDL3; a borderless
desktop fullscreen window reports `True`). Places only ever requests
borderless desktop fullscreen, so `Off` vs not-`Off` is exactly the mode
contract; exclusive mode would be identified with `display_mode().is_some()`.

SDL3 window operations are asynchronous requests. Places already observed
window state by polling once per frame (`refresh_display_status`,
`render_and_present`) and keeps doing so; the scripted resize sequence still
lands on the requested pixel sizes (see §8). The one place a barrier is used
is boot-time fullscreen: SDL2 created the window already fullscreen, so a
first-frame capture saw the display-size drawable, while SDL3 finalizes the
request after the window exists. `create_window` therefore calls
`SDL_SyncWindow` (`window.sync()`) once when the saved mode is fullscreen,
which restores the SDL2 first-frame state (verified: 1512x949 logical /
3024x1898 pixels on this machine, identical to SDL2). A timed-out sync is
logged and non-fatal; the per-frame poll adopts the state when it lands.

## 6. Events and input equivalence

The event surface is unchanged: `Event::Quit`, `Event::KeyDown`, `Event::KeyUp`
only. SDL3's `KeyDown`/`KeyUp` carry the same `keycode: Option<Keycode>`,
`scancode`, `keymod`, `repeat` fields plus two new ones (`which`, `raw`) that
Places ignores. The synthetic events in `src/input/tests.rs` were extended with
those fields.

Input remains **keycode-based** (logical, layout-dependent), never
scancode-based; no binding changed. `Keycode::name()` still supplies fallback
names, and the special spellings (`ESC`, `-`, `KP_MINUS`, `ENTER`, …) are
unchanged, so `settings.json` stays compatible. Key repeat semantics are
unchanged: the first `KeyDown` (`repeat: false`) presses or triggers, repeats
do nothing, `KeyUp` releases.

The SDL3 crate models `Keycode` as a real enum (SDL2's was a struct of
constants), so `keycode_to_str` and the menu-navigation mapping now use
explicit lookup tables instead of wildcard matches; the tables contain the same
mappings, and a future SDL3 key constant cannot silently change behaviour.

## 7. macOS metal view

Under SDL2 the raw-window-handle implementation reported the `SDL_MetalView`,
so `builder.metal_view()` was required before `build()`. Stage 12 determined
what SDL3 requires, with runtime evidence rather than assumption:

1. **SDL3 + raw-window-handle supplies the native view automatically.** The
   `sdl3` 0.20.0 implementation returns the window's content view
   (`[ns_window contentView]`) in `RawWindowHandle::AppKit`, regardless of
   `metal_view()`; wgpu's Metal backend makes it layer-backed and attaches its
   own `CAMetalLayer` through `raw-window-metal`, which tracks the view's
   bounds and backing scale.
2. **No window property or flag is required.** `SDL_Metal_CreateView` adds a
   separate, unused `CAMetalLayer` subview above the content view; it is not
   part of the handle wgpu presents to.
3. **The old workaround is obsolete.** Both configurations were launched and
   screenshotted: the window showed the same rendered scene, with identical
   pixel statistics, and both produced the same captures. The metal view is
   therefore removed, and with it the pre-build `render::apply_window_flags`
   hook (`main` now builds a plain `video.window(...)`).

The unsafe surface-creation contract is unchanged and lives in
`src/render/wgpu/surface.rs`: the two `unsafe` operations (raw-handle extraction
and `create_surface_unsafe`) are confined to one `unsafe fn`, the window is
declared before the renderer in `main`, and the surface is recreated from the
same window on loss. No new `unsafe` code, no transmutes, no leaked handles.

## 8. Verification

- Immediate SDL2 baseline captured from the pre-migration release binary at
  `target/agent-work/stage12/baseline-sdl2/` (25+25 canonical, 40+40 expanded,
  plus lifecycle/benchmark/UI evidence). The pre-migration binary's sha256 is
  recorded with it.
- The final SDL3 build captured with the same scripts and pinned state
  (`target/agent-work/stage12/sdl3-final2/`); comparison via
  `python3 tools/bench/compare_captures.py`: **50/50 canonical and 80/80
  expanded byte-identical** (mean difference 0.000, max channel 0, >8 share
  0.00%). UI/menu, after-restore and fullscreen captures are byte-identical
  too.
- Sizes, adapter, surface format, present mode, resize sequence, fullscreen
  state and minimize/restore are recorded in both evidence logs, and the
  SDL2/SDL3 fixed lines are identical: Apple M2 Pro / Metal /
  `Bgra8UnormSrgb` / `Fifo` / `Opaque` / `Depth32Float`, 640x360 -> 1280x720,
  800x450 -> 1600x900, 500x300 -> 1000x600, 1512x838 -> 3024x1676, fullscreen
  1512x949 -> 3024x1898.
- Runtime input matrix through real OS events: W/S/A/D displacement, arrow
  look at 90°/s horizontal and 60°/s vertical (measured 89.6/59.9), Escape
  pause/resume, W+S cancellation, no stuck key after release.
- `cargo test --workspace --all-features`: 979 passed / 0 failed / 5 ignored
  (same as the pre-migration baseline); the 5 ignored GPU/diagnostic tests pass
  when run explicitly.
- `sh tools/verify.sh` green, including the real-window bootstrap and compiled
  build suites and `cargo build --release`, plus an isolated clean release
  build and the two smoke suites run against it (`PLACES_SMOKE_BIN`).

## 9. Platform status

| Platform | Backend | Status |
| --- | --- | --- |
| macOS | wgpu → Metal | VERIFIED (runtime, this machine) |
| Linux | wgpu → Vulkan | NOT EXECUTED — ENVIRONMENT BLOCKED (Stage 13 gate) |
| Windows | wgpu → D3D12 | NOT EXECUTED — ENVIRONMENT BLOCKED (Stage 13 gate) |

Stage 12 does not claim cross-platform release validation, and it does not
change the desktop backend policy: SDL3 is the platform layer, wgpu remains
the only renderer.
