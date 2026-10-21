# Desktop verification

Places uses wgpu through SDL3: Metal on macOS, Vulkan on Linux and Direct3D 12 on
Windows. Source acceptance and native acceptance answer different questions.
Choose the smallest gate covering your change, then retain the actual command,
source/binary/package identities and limitations with the result.

## Prerequisites and source checks

The repository pins exact Rust 1.99.1 with Clippy/rustfmt in
`rust-toolchain.toml` for reproducible CI and release builds. Cargo's
`rust-version = "1.99"` remains the MSRV, not the selected build toolchain.
The optional [Places-local stable override](DEVELOPMENT.md#build-from-source)
can reuse an already-installed stable 1.99.1 without a duplicate fixed-version
installation. Before recording verification or comparisons, run
`rustc --version` and `cargo --version` and confirm both match the selected
repository pin; a moving `stable` label alone is insufficient evidence.
Historical reports retain the toolchain versions actually used in their dated
campaigns.

Native builds need SDL3 3.2+, `pkg-config` and a C++17 toolchain for vendored Luau; macOS uses
`brew install sdl3 pkg-config`. Linux needs SDL3
headers, the Vulkan loader and a driver. Windows needs SDL3 and its C/C++ toolchain.
Python tools require 3.10+; Pillow and NumPy are optional authoring/analysis
dependencies, as described in [offline tooling](OFFLINE_TOOLING.md). Real-window
checks require an available graphical session and the native backend.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --locked --release --workspace --all-features
PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'
python3 tools/assets/validate.py
```

`sh tools/check-rust.sh` runs the complete locked Rust gate: format, all-target /
all-feature check, strict debug and release Clippy, then all-feature production
builds. `Cargo.toml` and `clippy.toml` are authoritative for lint policy. Fix causes
rather than suppressing warnings.

[GitHub CI](../.github/workflows/rust.yml) performs those stages in a temporary
engine copy with optional map directories omitted, then runs maintained asset /
generator checks and source Python tests. It rejects tracked compiled packages.
Ordinary CI does not bake production maps or capture reflection probes. The old
broad Rust suite was removed in 0.11.0. The subsequent essential CPU regressions
run with `cargo test --workspace` (245 at the 0.13.0 final gate); Python contracts,
production validation and developer diagnostics remain active. Additional Rust tests require essential concrete regression value and proportional
cost, live under the owning engine/game/editor/map_compiler tests folder, and
must be wired into Rust discovery as described in `AGENTS.md`.

## Full desktop gate

```sh
sh tools/verify.sh
```

This opt-in gate builds fresh release tools, validates assets/generators, compiles
and verifies the `levels/lineup.txt` distributions, checks every collection sub-map
and its packaged weather, runs
the Rust production gate and enables export/native Python suites. The script is the exact command/order reference;
this guide does not duplicate its evolving command list. It stops on failure.
`PLACES_VERIFY_WORKERS` selects the offline worker budget (default eight).

`PLACES_VERIFY_ALL_MAPS=1 sh tools/verify.sh` additionally runs the explicit
recursive fixture package campaign. Its standalone equivalent is:

```sh
python3 tools/bench/regression_maps.py --run-packages \
  --compiler target/release/places-compile --out target/verification/new-map-campaign \
  --prepared-root target/verification/maps --install-source-packages
```

Use a new receipt directory and preserve useful results outside `target` before
cleaning. Historical Places Demo and Model Zoo JSON under `docs/archive/levels/`
are excluded from active discovery, lineup builds and default campaigns. Full
package acceptance can take many minutes; source/documentation checks do not
justify refreshing every installed map.

## Generator and asset checks

Read [asset contracts](ASSET_SPECIFICATION.md) before changing imagery/models and
[map authoring](MAP_AUTHORING_GUIDE.md) before changing placement or level data.
Use each source owner's read-only `--check`; the [map index](../levels/README.md)
and [offline tooling owner table](OFFLINE_TOOLING.md#source-ownership-and-export)
identify the right tool. Checks prove their named structural/byte contracts,
not full native appearance.

```sh
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 tools/entities/check_clip_boundaries.py
python3 tools/levels/compile.py --list-json
```

Do not run broad artwork exporters or `--force` as a validation shortcut. The
texture dimension guard does not protect same-size approved art from replacement.
Generate deliberate changes in scratch first, review them and test repeatability.
Runtime imagery must remain real committed PNGs, with source/UV/alpha contracts
preserved. Model construction and code-generated test controls are separate from
runtime texture generation.

For changed collection sources, export the complete project to its explicit destination:

```sh
./target/release/places-compile export-project projects/environments \
  --out assets/levels/environments.placesmap --asset-root assets --workers 8
./target/release/places-compile validate assets/levels/environments.placesmap
./target/release/places-compile verify projects/environments --level office \
  --package assets/levels/environments.placesmap --asset-root assets --require-current
python3 tools/levels/compile.py --check
```

`validate` decodes and re-hashes every bundle record; `verify --level` checks the
selected sub-map’s source/dependency and compiler currency. The curated tool runs
that verification for every declared sub-map and checks each Static/Full manifest.
`tests.built_maps` independently checks every installed sub-map’s prepared payloads,
content hashes and exact embedded editable sources without baking. Source
`tests/test_package.py` does not require packages.
Current packages prepare Static (`off`) and Full lighting; legacy Medium remains
readable. Texture quality/filtering preserve selected lighting. Current encoding,
geometry/solver revisions and safety limits live in [package format](PACKAGE_FORMAT.md)
and implementation constants rather than duplicated dated inventories.

## Empty installation and external delivery

Zero maps is a supported installation with a complete resource payload. Missing
required assets are a separate startup error. The player creates platform user
storage, not writable directories beside the executable; the cache directory is
reserved for diagnostics/future cache users and is not a lightmap bake.
`PLACES_LEVEL` naming a missing map returns to the menu with Explore/import advice.
Use the explicit native acceptance tool after building and staging a map-free
`--with-compiler` package:

```sh
python3 tools/verify-installation.py --installation /tmp/export/Places \
  --project local/verification/0.12.0-task1/project \
  --package local/verification/0.12.0-task1/project/packages/stage1_entry.placesmap \
  --out local/verification/installation-check
```

It copies the payload, isolates state and records direct native captures, import
and restart discovery, override precedence, read-only installation and unwritable
storage failures. It uses only the tiny compatible project/package; it never
bakes a demo or opens a desktop app handler. Existing `cases/` is refused to
preserve earlier evidence. See [paths and installation](INSTALLATION.md) for
platform conventions and tool limits. Ordinary unit tests never run this tool;
`PLACES_SKIP_SMOKE=1` excludes other optional native suites in source checks.
A skip is not native acceptance.

`sh tools/package.sh OUTPUT_DIRECTORY` exports the map-free player. Explicit
`--maps PACKAGE_DIRECTORY` includes external packages after validation/discovery
checks; packaging never bakes maps or consults the source lineup. Destination
and symlink guards protect existing source and user data. See
[map independence](reports/release-0.8.0.md#map-independence) for the historical acceptance.

## Native capture, loading and movement

The [benchmark guide](../tools/bench/README.md) describes camera campaigns,
compiler comparisons, traces, weather, movement and resource measurements.
Use compatible explicit packages and a fresh state/output directory; confirm
committed world/quality, renderer backend, successful presentation or capture,
nonzero world draws and no validation/device errors. Save binary/package hashes.
A screenshot does not establish ordinary gameplay FPS or exhaustive traversal.

`capture_art_style_hero.py` now defaults to a compact architecture fixture.
Frozen Demo camera scripts and GLES2 comparisons require preserved matching
historical packages; they cannot obtain Demo by building today's lineup.
The former renderer's 50 PNG inputs remain read-only in
`tests/fixtures/native/gles2-reference/`. [Renderer reference](RENDERER_REFERENCE.md)
explains the preserved implementation and comparison limits.

`loading.py` measures an already-built player serially with isolated cold/warm
application state, pinned window/quality/camera and `PLACES_LOAD_TRACE` JSONL.
Use `--install-package` for explicit fixture/historical packages. It does not
purge OS/driver caches. Missing or occluded presented samples remain unavailable,
never zero. Its startup, package-loading, first-usable-scene and settings-transition
metrics are distinct from steady-state frame timing and physical input latency.

Window/quality controls include `PLACES_BENCH_WINDOW_CYCLE`,
`PLACES_BENCH_QUALITY_CYCLE`, `PLACES_BENCH_GRAPHICS_CYCLE`, `PLACES_MOVE_SCRIPT`
and `PLACES_CAPTURE_TIME`. Native smoke action scripts use
`PLACES_BENCH_ACTIONS` with `start`, `request`, `upload`, `ready` or `failed`
anchors; `PLACES_PREPARE_DELAY_MS` is benchmark-only. Movement controls drive real
held actions, while `PLACES_SPAWN` / `PLACES_CAMERA` provide fixed inspection views.
Synthetic OS keyboard acceptance requires a frontmost application. A run that did
not receive those events cannot be reported as manual-input coverage.

## Interpreting diagnostics and platform evidence

Normal player launches retain `diagnostics/player.log` and
`diagnostics/player.previous.log` below the same state root as `settings.json`
(or `PLACES_STATE_ROOT`). Each log is capped at 256 KiB, each record at 4 KiB;
rotation preserves the session identity and latest runtime context. A small
`session.active` marker distinguishes completed shutdown from an abrupt stop.
One locked `owner.lock` file prevents a second player from rotating an active
session's evidence. A concurrent launch or unwritable directory falls back to
stderr and reports that persistent evidence is unavailable.

Logs contain startup version/platform and exact executable SHA-256, adapter,
map path and manifest identity, loading/cancellation, window and graphics
transitions, warnings, and a 15-second state/RSS/GPU-timing context sample.
No credentials, full environment, image buffers or OS reports are collected.
Panic messages/location and a bounded backtrace are recorded by a panic hook;
the ordinary fatal behavior remains. Returned fatal errors show a readable SDL
dialog. Explicit `PLACES_BENCH=1` runs retain the same evidence on stderr/disk
without blocking unattended tooling on dialogs.

An interrupted-session notice means the prior process did not finish normally;
it cannot distinguish a native crash, force quit, power loss or external signal
by itself. For macOS native failures, open Console > Crash Reports and select
the report for `places` at the matching session time. Share the two small player
logs and the relevant sanitized report excerpt. Review local paths before
sharing. No signal handler attempts unsafe logging/UI, and no panic is swallowed.
Allocation aborts, SIGKILL and failures before logging initializes may leave
only the last context and OS evidence. Read-only or full disks can prevent even
that evidence; a log write failure disables further disk writes for the session.

Drawable-size guards and actionable fatal errors remain in the production
renderer. The former ignored Rust GPU controls were removed with the Rust suite;
use direct native player diagnostics for scoped renderer acceptance.

Texture checks distinguish preferred-size/manifest advisories from hard format,
alpha, aspect and budget failures. Accepted source art may have advisories; report
actual output rather than relying on an old fixed warning count. Package tests
intentionally exercise bad catalogs and print expected failure diagnostics after
a successful unittest summary. Inspect exit status and the named negative fixture.

Native diagnostics are explicit developer workflows, outside ordinary CI.
Use the smallest direct executable campaign covering the changed behavior.

Recorded native campaigns on the development host use M2 Pro/Metal. Historical
Linux container runs used software Vulkan; Windows cross-builds were not native
execution. Those results are limited to their dated reports, including
[feature expansion](reports/release-0.8.0.md#feature-expansion-summary),
[Art-style integration](art-style/stage7/README.md) and
[Office repairs](reports/release-0.8.0.md#office-defect-repair). They do not establish current
Linux/Windows physical GPU acceptance. A source CI pass establishes its source
checks, not every installed package's currency or manual keyboard/mouse feel.

## FPS comparison requirements

Disable V-sync and every engine/tool frame limiter for all FPS comparisons.
Record the actual presentation mode and physical drawable size for each side,
and verify that display pacing does not cap the result. Requested Immediate mode
alone is insufficient: a run plateauing at refresh rate is inconclusive for
throughput gains and must be labeled accordingly. Report the observed pacing
limit; do not turn GPU stage timings or a requested setting into an FPS gain.
