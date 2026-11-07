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

[GitHub CI](../.github/workflows/rust.yml) runs test-aware check/Clippy/CPU gates
in the full source checkout so maintained quest Luau modules remain available.
Formatting and production binaries use a temporary engine copy with optional map
directories omitted. Maintained asset/generator checks and source Python tests
run in the full source checkout. It rejects tracked compiled packages.
Ordinary CI does not bake production maps or capture reflection probes. The old
broad Rust suite was removed in 0.11.0. The subsequent essential CPU regressions
run with `cargo test --workspace` (360 on the final 0.13.0 Razer gate;
362 on the accepted Mac gate); Python contracts,
production validation and developer diagnostics remain active. Additional Rust tests require essential concrete regression value and proportional
cost, live under the owning engine/game/editor/map_compiler tests folder, and
must be wired into Rust discovery as described in `AGENTS.md`.

## Feature-work map preparation budget

During feature work, prepare only maps or the smallest deterministic fixtures
actually used to validate a named behavior. Record the feature, whether the run
was executed or merely planned, and whether Static preparation is sufficient.
Use Full solves for lighting-dependent checks and the final-build/export contract;
reuse already current accepted outputs instead of rebaking them. Embedded editor
testing deliberately needs no final lighting bake.

Do not refresh the complete release inventory or run a broad startup sweep just
to assemble a feature-validation download. Defer the complete release-map set to
the final validation stage. Such work does not itself authorize a main push, tag
or release. Preserve completed exports and useful interrupted staging evidence
when scope changes, and label any partial distribution **validation-only**.
Final release packages retain their existing intact Static/Full, dependency,
source recovery and currency requirements; this budget does not weaken them.

## Scoped live-cinematic acceptance

Use a private, compact two-level project, not a new production map campaign.
In one level, an ordinary player trigger starts a cinematic sequence containing
a bounded NPC Luau action and Move, live Follow/Look-at and Orbit shots. Export
the complete project with Static/Full variants, validate it, and verify both
levels current. Confirm `live-cinematics-v1` in the manifest. Play the package
on the actual native backend and inspect captures from the remote room while
the physical player stays behind. Record the shared effective camera/listener,
NPC motion, unchanged body/picking/flashlight ownership, and exactly one keyed
completion reaching the second level.

In a separate short run, pause and compare unchanged simulation/camera samples,
resume, cancel through ordinary `stop_sequence`, verify fresh input works,
reenter, then return to the menu during the camera lease. Check that no camera,
actor or world sound survives unload. Use muted private settings if sound is
not under perceptual review; listener/pan/path telemetry is not human listening.

Exercise the actual visible Advanced Cinematics controls on an isolated project:
capture start/endpoint, order a Script/Move/Wait, choose a destination and Apply.
Compare one Undo/Redo, saved source and unchanged sibling/script data. Embedded
Test must use the remote view/listener, hide player targeting chrome during the
lease, report its existing cross-level boundary explicitly and restore editing
without changing the document or editor view. Repeat Test/Stop with fresh state.
Use the ordinary final package for actual travel, not a new embedded loader.

Focused CPU regressions cover malformed/bounded/optional data, lease identity,
target generation, pause/cancel/reentry, input versus live physics, full-queue
completion retry without replay, capability admission, camera-only bake reuse,
and atomic/stale-safe editor wiring. These need no renderer or full map bake.
Retain exact binary/source/package identities and distinguish executed evidence
from plans. Native Windows and broad performance/release campaigns remain
separate gates; do not infer them from macOS or refresh unrelated renderer proofs.

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

`sh tools/package.sh OUTPUT_DIRECTORY` exports the map-free player. Every export
validates the bundled menu against final staged assets and current menu source
using the matching release compiler; it does not bake maps. Explicit
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

### Explicit animated transparency controls

`python tools/bench/character_transparency_inputs.py --out NEW_DIRECTORY` writes
nine small #24 controls for idle/float, glass/water/planar interactions, absence,
fade reversal and a 32-actor cost view. Compile them explicitly with Static/Full
variants into an isolated state root. This tool never bakes maps or runs in CI.
The original exact rear view remains in `tests/fixtures/native/issue24-assets/`.
`PLACES_BENCH=1 PLACES_BENCH_CHARACTER_BATCH=off` retains general triangle
submission for a matched oracle. Follow the ordinary uncapped FPS requirements
below; functional Fifo screenshots do not establish throughput. See the
[Windows evidence and commands](reports/issue24-transparency-windows.md).

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

### Complete bloom-footprint native control

With an all-feature developer player, `PLACES_BENCH=1 PLACES_BENCH_BLOOM_PROBE=1` runs eleven tiny numeric attachment controls on the
already-created native device before ordinary loading. It exercises every
row/column phase of an 8×8 source, 7×5 and 13×9 trailing edges, constant energy
and a 1×1 source. It uses the actual prefilter pipeline and checks GPU readback
against independent area coverage with a one-byte output tolerance. It creates
no artwork, map bake, extra adapter or ordinary frame-time workload. A validation
or numerical failure returns an error; successful controls log `[bloom-probe]`.
This diagnostic run must not be used for FPS acceptance.

### Original hollow-metal exit-door integration

Run `python3 tools/doors/check.py --check` for exact source/export identities and
preset contracts. [The dated Mac report](reports/exit-doors-0.13.1-mac.md) separates
actual native/editor acceptance from Blender previews, documents room-light
casting limits, and gives the small explicit fixture recipe for Windows follow-up.
No full-map or broad model-review repeat is required for this feature gate.
