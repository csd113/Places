# Settings and input correctness — 0.11.0 staging

Based on `68601b3b40a77f62ab8fcb046c55eefe1d9fa14f` on the user's Mac,
following the completed renderer, shader, physics, sound and loading passes.
The existing settings model, Controls page and native event loop remain the
owners; no UI redesign, input framework or new dependency was introduced.

## Confirmed defects and corrections

- **Partial-file data loss.** Missing mandatory settings or early binding fields
  invalidated the entire JSON object, discarding otherwise valid preferences.
  Missing fields now receive their normal defaults. Explicit graphics choices
  remain independent: low texture quality can retain Full lighting. A false
  `PLACES_NO_BLOOM` switch no longer forces a saved-Off bloom preference on.
  Control/FOV sanitization also replaces nonfinite values before clamping.
- **Unreachable and conflicting keys.** Validation accepted unknown key names
  and reserved `KP_MINUS`, while native SDL events emitted `KEYPAD -`. A shared
  native-name resolver canonicalizes aliases and rejects unreachable names.
  Pause, overlay and diagnostic keys retain shell ownership. Repair reserves
  valid configured keys before assigning unused defaults, and retains explicit
  unbound entries across reloads rather than stealing another action's key.
- **Console ownership.** The console mixed a fixed physical Grave scancode with
  logical gameplay bindings. Console is now an ordinary configurable logical
  binding, defaulting to backtick, using the same conflict validation as
  Interact and Flashlight. Rebinding it releases its previous key. The existing
  Controls row count derives from the binding list so its Back row stays reachable.
- **Repeat capture.** Controls accepted OS repeat events as the next binding,
  allowing a held menu activation key to bind itself. Gameplay, menu navigation,
  console toggles and rebinding now share one fresh-press helper. Text editing
  retains its normal repeat behavior; short gameplay taps retain their press edge.
- **Focus and paused clocks.** Clearing held keys did not stop airborne player
  motion, rigid bodies or visual updates during focus loss. The preserved
  baseline fell from 9.6 m to 5.09 m while reporting no focus. One activity gate
  now stops player/world, physics, weather and visual clocks for focus loss,
  menus, console and loading, with timing reset on focus changes. World time
  scaling remains separate from player motion and geometric carry control.
- **Persistence publication.** Direct writes could truncate the previous file
  before a save completed; a second malformed file replaced the previous invalid
  backup. Saves now write/sync an exclusively created sibling before rename.
  Failure removes only the owned temporary. Malformed files retain separately
  numbered backups using non-replacing publication.

## Regression value and verification

Three additional CPU regressions live in the owning `src/game/tests/` tree:
one protects partial-file/save failure and successive backup data loss; one
protects native key identity, binding repair and unbound/console ownership; one
protects the shared press/repeat/tap/reset contract. They use tiny JSON,
temporary settings files and SDL event values, with no native device, compiled
map or shipped asset dependency. Native lifecycle acceptance remains explicit
developer tooling, outside Rust tests and ordinary CI.

Validation receipts, initial failures, the preserved baseline reproduction,
action scripts, settings, native traces/captures, executable hashes and final
exact-commit CI/cleanup/custody receipts are kept
outside `target` in `local/settings-input-0.11.0/`.

The opt-in `tools/bench/settings_input.py` launches a direct executable against
an existing developer-console fixture. It covers repeat/rebind, independent
graphics choices, flashlight/interact edges, console and pause capture, saved
binding reload, focus transitions and stopped-world player/carry control. It
never builds or bakes. The native key action's optional `repeat` flag allows the
same handler path to receive explicit OS-repeat cases.

The complete `RUSTC_WRAPPER= sh tools/check-rust.sh` gate passes: formatting,
all-target/all-feature check, strict debug and release Clippy, `cargo test
--workspace` (54 passed, zero failed/ignored, 0.06 seconds), and the locked
all-feature production build. Python source contracts pass 372 tests with
17 explicit skips in 95.525 seconds (`PLACES_SKIP_SMOKE=1`). The new regressions
increase the previous CPU count from 51 to 54.

The final direct executable completes all 18 native assertions across three
processes, 139 scripted actions and 17 captures. The existing Controls layout
and carried-cup captures were visually inspected. Low textures retain actual
Full renderer lighting, and false `PLACES_NO_BLOOM=0` retains saved bloom-Off.
Rebound controls persist into the next process. Pause/resume retains an exact
stationary position after a key release in the menu. Focus loss retains the
9.558 m airborne height after the pre-loss simulation interval; the baseline
continued falling. At world scale zero the player and held cup each move about
0.600 m together while the solver's fixed-step count remains zero. All three
native processes reach `shutdown_complete`. These runs report actual Fifo
presentation at 1280 × 720 physical pixels, with a 480 × 270 Low scene target.

All four production executables and SHA-256 receipts are preserved under
`local/settings-input-0.11.0/bin/`; the player SHA-256 is
`c2e0d07c892a1339fb95629be46ec2b5a13733aaaaae133c14d89c97896ce90a`.

## Limits and stage boundary

Native verification is macOS/Metal only. Scripted focus events exercise the real
focus handler and SDL capture reconciliation; they do not certify OS focus
switching on every desktop. Functional runs retain V-sync, so no FPS comparison
or throughput improvement is claimed. Staged settings writes protect against
failed publication; file synchronization alone is not a guarantee against every
filesystem/power-loss failure. Backup/staging slots are bounded and their
publication failures retain diagnostics rather than promising unlimited recovery.

No shipped map or asset was changed or baked. No accompanying settings/input
issue is open; broader UI, distribution and platform issues remain open. Main
merge, new branches, releases, entity/navigation, editor, packaging, map-format
reset, broad compiler changes and flowing water remain outside this pass.

Final custody is recorded in `local/settings-input-0.11.0/CUSTODY.md`: preserved
executables are verified before cleanup, all build/native users exit before
`cargo clean`, and checkout/index/build/native-window custody is explicitly
released when the exact pushed-commit CI and cleanup receipts are complete.
