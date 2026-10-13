# Crash investigation and bounded diagnostics — 2026-10-13

Two renderer invariant gaps are corrected and failures now retain bounded
support evidence. **The user's intermittent gameplay crashes remain unattributed;
this work does not establish that they have been eliminated.** No release/version
integration, production-map bake or visual-asset change was performed.

## Evidence and confirmed corrections

The available macOS report is a `places-compile` SIGSEGV on October 12:
`EXC_BAD_ACCESS`, invalid address `0x8`, main-thread CoreFoundation bundle access
through IOSurface and Metal device discovery (`MTLCopyAllDevices`). It is a
native signal, not a Rust panic. The report's compiler frames are stripped; its
binary SHA, map and settings were not retained. Older Places crash metadata
exists without its report; a separate debug-player record names a force quit.
Neither that evidence nor the temporal proximity of an earlier `cargo clean`
establishes the gameplay cause. Raw OS reports and unrelated private files were
not copied into repository evidence.

**Oversized drawable.** The requested wgpu device limit is 8192 pixels per edge,
but the renderer previously accepted any drawable and allocated its depth
attachment without checking that limit. A 64×64 native control confirmed that
an 8193×64 depth request generates a validation error. Initialization, resize,
attachment preparation and capture now reject oversized physical drawables
before allocation, with the observed size, limit and smaller-window remedy.
Zero-sized minimized windows remain valid. A real 4097×64 logical window on the
2× backing display produced the expected readable fatal error and clean exit,
without a validation panic.

**Window lifetime.** The safe constructor returned `Surface<'static>` without
borrowing or retaining its SDL window. Although the player already declared
locals in a safe order, another safe caller could drop its window first,
violating the raw-handle contract. The renderer now retains the SDL window's
shared context as its final field, after the surface and GPU resources. Lost
surfaces are recreated from that exact retained window. This is a demonstrated
API safety hole; it is not an attribution of the observed compiler SIGSEGV.

The audit found no confirmed timestamp-callback or level-unload use-after-free.
Callbacks capture channel senders, and the three timestamp slots remain owned
through completion, mapping and unmapping. Existing cancellation and displayed
world leases preserve independent resource ownership.

## New diagnostics

`<state-root>/diagnostics` uses the same state root as settings, including
`PLACES_STATE_ROOT`. Two logs are capped at **256 KiB each**, records at **4 KiB**.
Rotation retains executable/version identity, the latest map request and runtime
context. A locked owner file prevents a concurrent player from rotating another
session. An active-session marker is removed after completed shutdown.

Quiet launches retain exact executable SHA-256, platform/version, adapter,
requested package manifest identity/path, loading/cancellation, window/graphics
transitions, warnings and state/RSS/GPU-timing context on state changes or every
15 seconds. No credentials, environment dump, image buffers or OS reports are
collected. Panic location/message and a bounded backtrace are recorded without
catching or continuing the panic. Returned fatal errors request a readable SDL
dialog. Explicit benchmark runs retain the evidence without modal dialogs.

A missing shutdown marker cleanup triggers a next-start notice describing an
interrupted session, which may mean a crash, force quit or interrupted shutdown.
A controlled SIGTERM was handled gracefully by SDL and generated no notice;
SIGKILL retained the marker, and restart reported the interruption, preserved the
previous log and completed normally. The desktop approval/session interruption
prevented reliable visual acceptance of the modal notice; the notice call and
text were verified from persistent evidence. No approval remains pending.
Native signals, allocation aborts and SIGKILL cannot safely promise immediate UI;
OS reports and the last session context remain necessary. Failed log I/O disables
further disk writes; read-only/full disks and very early failure limit evidence.

## Validation, cost and scope

- `cargo fmt --all --check`, strict workspace/all-target/all-feature Clippy in
  debug and release, and `git diff --check`: passed without weakening lints.
- `cargo test --workspace`: 2,263 library plus nine integration tests passed;
  library runtime 33.70 s. All-feature workspace tests: 2,269 plus nine passed,
  library runtime 31.44 s. Nineteen explicit native/performance tests remain
  excluded from automatic tests; the new small native limit control passed in
  0.05 s. No normal Rust test or CI stage bakes a shipped map.
- Source Python checks: 354 tests passed in 128.57 s, four native classes
  explicitly skipped; native controls above were separate real-window runs.
- New deterministic coverage protects Unicode/record bounds, rotation, context
  preservation, clean/interrupted shutdown, competing writers and both drawable
  edges. Its isolated storage fixture writes less than 1 MiB. The existing
  ordinary-wall regression now presses while grounded before jumping against
  the same contact. Existing shared ramp/stair and movement controls passed.
- Native M2 Pro/Metal: 480 ready frames in 9.44 s exercised movement,
  resize/minimize/restore, Low/Medium/High textures, Static/Full lighting,
  filtering, bloom and reflection preferences, with 414 GPU timing samples.
  Office has no reflective draws; this does not repeat physical planar/probe
  acceptance. Pause/resume, world replacement, cancellation, failed preparation
  with the prior world retained, skipped-present shutdown and oversized-window
  exit passed. No device-loss injection or exhaustive gameplay traversal is
  claimed.
- Only the existing one-room `graphics_fixture_contract.json` was compiled:
  865 ms, 236,100 bytes, Static/Full, integrity and `--require-current` passed.
  All eight installed production packages remain byte-identical. Office was
  runtime-compatible; the other seven were correctly rejected for their known
  stale catalog identities. Their refresh and supported-package reconciliation
  remain for the final release stage.
- Native diagnostic logs were 5.6–26.4 KiB per short control. Startup elapsed
  through executable identification was 26–42 ms. Steady operation adds one
  elapsed-time check per frame and low-frequency context writes; no comparative
  FPS improvement or zero-cost claim is made.

Final native player SHA-256:
`329c8d23c1a6fb45946fffdb7af7f8786280d6fb6a72abeebb412fd59aea6ab9`.
Small private receipts and recipes remain in `local/crash-investigation`, outside
`target`. The three unknown-owner audits, cloud entity candidates, assets, maps,
version and root README were preserved. Exact publication/CI, scoped cleanup and
custody release are recorded in the final handoff and local verification receipt.
