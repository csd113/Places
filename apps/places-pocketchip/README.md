# Places for PocketCHIP

**0.11.1** · `io.vitrallis.liminalrust` · native Rust · PocketCHIP ARMv7.
[Release notes](CHANGELOG.md).

Explore an office, empty pool and unfinished interior in a first-person walking
game. This PocketCHIP edition replaces the older Liminal package while keeping
its application ID, so App Manager treats it as an update.

## Controls

Menus use `W` and `S` to select, `ENTER` to activate and `ESC` to go back.
Settings values use `A`, `D` or `ENTER`. Controls are discoverable from the main
and pause menus. Gameplay defaults are:

| Action | Key |
| --- | --- |
| Walk forward/back | `W` / `S` |
| Strafe left/right | `A` / `D` |
| Look up/down | `UP` / `DOWN` |
| Look left/right | `LEFT` / `RIGHT` |
| Pause, then exit through the menu | `ESC` |
| Performance overlay | `-` |

All eight movement and look bindings can be changed in Settings → Controls.
Restore Defaults recovers the key map. Level Select starts Places Demo, and
`ESC` returns to the pause menu for settings or exit.

## Runtime and storage

The ARMv7 EABI5 hard-float executable requires the device's SDL2, X11/EGL/GLES2,
GNU libc and Mali/Lima graphics stack. It is built for the Cortex-A8 and targets
the PocketCHIP's 480 × 272 display. It does not install system libraries at
launch. The game uses an OpenGL ES backbuffer and presents completed frames
through SDL's swap interval when supported; the device profile and measurements
are documented in the [source tree](../../source/places-pocketchip/docs/POCKETCHIP.md).

The package contains `assets/catalog.json`, the Places Demo level, textures,
models and the ARMv7 executable. Asset textures are scaled to a 384-pixel
maximum dimension for the App Manager package size limit; the source artwork
remains in the [development tree](../../source/places-pocketchip/). Settings,
custom levels, imports and the lightmap cache are writable state outside the
published file inventory. Storage permission is required for that state.

This release has been validated as a catalog package and cross-compiled against
the PocketCHIP SDL2 sysroot. Device installation, update and visual checks must
be recorded separately before enabling a new installable release.
