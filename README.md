# Liminal

A slow first-person walking game for the PocketCHIP: quiet residential
interiors that keep going, built from rectangular rooms, hallways and a baked
static lighting system that runs on Mali-400-class hardware through OpenGL ES
2.0.

There is nothing to collect, fight or solve. You walk, and the building
changes around you.

## Levels

Three large, hand-authored residential levels ship with the app:

| Level | ID | Setting |
| --- | --- | --- |
| The Residence | `the_residence` | One very large house: entrance hall, living rooms, kitchen wing, bedroom corridors, service rooms and a back wing that has been leaking for years. |
| Quiet Apartments | `quiet_apartments` | An apartment building whose corridors and apartment interiors run into one another; the far apartments are only reachable through their neighbours. |
| After the Leak | `after_the_leak` | A house with a long-standing water problem spreading out of its service core; the last rooms are soaked and lit by two surviving fixtures. |

Each level is maintained near the spawn and decays as you walk: water staining
creeps along walls and ceilings, carpets turn damp, fixtures fail one by one and
furniture drifts out of place. The change is gradual, and the far end of each
level is dark but never unreadable. Older development levels (Level 1, the
asset demo, and the prop showcase/stress fixtures) are still installed and can
be chosen from the same menu.

## Controls

Menus use `W`/`UP`, `Z`/`DOWN`, `A`/`LEFT`, `S`/`RIGHT`, `ENTER` to activate and
`ESC` to go back.

Gameplay uses these bindings (all of them can be changed in Settings):

| Action | Key |
| --- | --- |
| Walk forward | `W` |
| Walk backward | `Z` |
| Strafe left | `A` |
| Strafe right | `S` |
| Look up | `O` |
| Look down | `.` |
| Look left | `K` |
| Look right | `L` |
| Pause menu | `ESC` |
| Performance overlay | `-` |

The overlay prints frame timing, CPU/GPU load, submitted draw calls and the
baked-lighting summary; it is hidden by default.

## Running it

Launch **Liminal** from Vitrallis App Center, or run the packaged binary
directly:

```sh
bin/armv7-unknown-linux-gnueabihf/app
```

The app resolves its levels, props, imported level packs and its `settings.json`
relative to the package root, which it derives from the installed executable
path (`bin/<target-triple>/app`), so it does not need to be started from any
particular directory. A development build launched with `cargo run` from this
directory uses the crate path instead.

## Level packs

`levels/*.json` (and `levels/*.zip` level packs) are installed by copying them
into the `levels/` directory inside the package. Levels are validated on load;
a level that fails validation is skipped and reported on the console rather
than crashing the game.

## Runtime prerequisites

- ARMv7 hard-float Linux with glibc 2.36 or newer (PocketCHIP).
- SDL2 2.26.5 or newer, with an OpenGL ES 2.0 driver (Mali-400/Lima on the
  PocketCHIP). Windowed SDL2 and an X11 session are required; the app sets its
  X11 window class to `io.vitrallis.liminalrust`.
- No network access, no audio device and no Python runtime are used. The app
  writes its `settings.json` next to its own package files, which is why the
  manifest declares the storage permission.

## Building

```sh
cargo build --release                  # development build for this machine
cargo test                             # level, lighting, renderer and format tests
```

The PocketCHIP payload is cross-compiled and staged by the repository tooling
against an image-matched SDL2 sysroot; see `VITRALLIS_APP_BUILD_GUIDE.md` and
`docs/experimental-rust.md`. Cargo is never run on the device.

## Level format

The format is documented in `liminal-design.txt` (rooms, walls with door and
window openings, per-room and per-wall material overrides, floor patches, ceiling
lights and placed props) and is the same format the bundled level editor writes.
