# Places

Places is the standalone home for Liminal, the game described below. Its Rust
crate, game assets, editor, tests and development tooling live at the repository
root so the project can evolve as a normal desktop game.

A slow first-person walking game: quiet residential interiors that keep going,
built from rectangular rooms, hallways and a baked static lighting system.

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

Menus use `W`/`S` or `UP`/`DOWN` to move through items, `A`/`D` or
`LEFT`/`RIGHT` to adjust values, `ENTER` to activate and `ESC` to go back.

Gameplay uses these bindings (all of them can be changed in Settings):

| Action | Key |
| --- | --- |
| Walk forward | `W` |
| Walk backward | `S` |
| Strafe left | `A` |
| Strafe right | `D` |
| Look up | `UP` |
| Look down | `DOWN` |
| Look left | `LEFT` |
| Look right | `RIGHT` |
| Pause menu | `ESC` |
| Performance overlay | `-` |

`Restore Default Bindings` in Settings puts this layout back after a rebind.
Custom bindings are saved to `settings.json` and are kept across launches.

The overlay prints frame timing, CPU/GPU load, submitted draw calls and the
baked-lighting summary; it is hidden by default.

## Running it

On macOS, install SDL2 and `pkg-config` if they are not already available:

```sh
brew install sdl2 pkg-config
```

Then launch the desktop development build from the repository root:

```sh
cargo run
```

The game resolves its levels, props, imported level packs and `settings.json`
relative to the repository root for development builds.

## Level packs

`levels/*.json` (and `levels/*.zip` level packs) are installed by copying them
into the `levels/` directory inside the package. Levels are validated on load;
a level that fails validation is skipped and reported on the console rather
than crashing the game.

## Desktop prerequisites

- macOS with SDL2 2.26.5 or newer and an OpenGL-capable driver.
- `pkg-config` so `sdl2-sys` can find the installed SDL2 library.

## Platform packaging

The game itself is platform-neutral. Historical PocketCHIP/Vitrallis packaging
artifacts are isolated in `platforms/pocketchip/vitrallis/`; they are not part
of the macOS development workflow. PocketCHIP packaging will be revisited as a
separate adaptation effort.

## Building

```sh
cargo build --release                  # development build for this machine
cargo test                             # level, lighting, renderer and format tests
```

Run the game and its tests from the repository root. Platform-specific build
and release steps belong under `platforms/` rather than in the game crate.

## Level format

The shipped JSON examples in `assets/levels/` demonstrate rooms, walls with door
and window openings, per-room and per-wall material overrides, floor patches,
ceiling lights and placed props. The bundled level editor writes the same format.
