# Carry-obstruction advisory / macOS acceptance

The text reported approximately as “listen cannot follow here” is exactly
**The item cannot follow here.** Its source is the carrying HUD in
`src/game/app/item_runtime.rs`, invoked by `FrameLoop::append_play_labels` in
`src/game/app/mod.rs`. It is not emitted by elevator loading, NPC following,
audio listening or navigation.

`ItemWorld::held_movement_blocked` in `src/engine/physics/hold.rs` reports a
sustained collision-limited carry: the checked body makes effectively no progress
while more than 0.3 m from the ideal hand goal, for over 0.35 s. The advisory was
previously drawn unconditionally during ordinary play. It was intentional short
feedback (see the historical 0.13.0 report), rather than a level-load error, but
its visibility did not follow the requested developer preference.

The preserved prior direct binary reproduces the exact text with Developer
Console off. In the tiny passenger fixture, pick up the box inside the closed
cabin, turn toward the doors and try walking. The retained box is obstructed near
the side rail; it stays held at a collision-checked pose instead of following
through scenery. This establishes the HUD source and one actual triggering case,
not the exact input history of the user's earlier live demo. A preliminary
camera turn did not free the body and is not counted as recovered clearance.

The advisory now follows **Settings > Controls > Developer Console**, default
off. It appears with that setting enabled without requiring an open console;
disabling the setting hides it immediately. The setting does not change swept
carrying, player/item constraints, safe drop/throw refusal, ownership or elevator
transport. An explicit failed release still shows **Make room to set this down.**
in ordinary gameplay. Empty hands show neither carry advisory nor stale release
feedback. Existing subtitle displacement remains intact.

## Reproduction

`tools/bench/carry_hint.py` accepts the explicitly exported `straight` source
from `elevator_cargo_fixture.py`. It does not build Rust, author a map or bake
lighting. Run against a direct game executable with a fresh `--out` directory,
serializing native sessions. It retains the starting pose, ordinary key/camera
actions, captures, trace checkpoints, package/binary hashes and per-case receipt.
The production setting path is toggled off/on/off while the same object remains
blocked; the final ordinary release and empty-hand developer view are checked.

Baseline and candidate evidence stays under `local/carry-hint/`. No elevator
artwork, wall placement, editor placement, door animation or transport code is
changed. The existing tiny fixture package can be reused because this game-only
change does not alter compiler preparation identity. No shipped map is rebaked.

## Source checks

Tested base: `3fee560e`. Strict workspace/all-target/all-feature Clippy passes.
All **445 workspace tests** pass (approximately 1.12 s test runtime, with builds
separate). One new CPU-only sphere/door fixture protects the actual obstruction,
off/on/off HUD visibility, unchanged held custody, collision refusal and recovery
when the obstruction clears. It needs no assets, native device or map bake and
takes approximately 0.02 s. The existing failed-release test still checks ordinary
feedback and stale-warning cleanup with developer diagnostics disabled.

The locked workspace/all-feature release build, formatting, Python compilation
and diff whitespace checks pass. The compiler verifies the existing straight
passenger package as current and integrity-valid, with no bake required.
The accepted direct game binary is retained at
`local/carry-hint/production/places`, SHA-256
`ee03f543d944555889bfb7ffb23fad09fcb61dff0be34a0fe1953b3474de05f8`.

## Native result

The accepted `native-final/` run passes all six checks. In the same closed Ground
cabin, with the same held key and unchanged blocked body pose, normal mode has
no advisory, developer-enabled mode shows the exact text, and disabling hides it
again. The console stays closed and gameplay stays live throughout. An ordinary
E press then releases the body; the empty-hand developer view shows no advisory.
The player remains in the same level and no destination is selected.

Actual game captures `state/blocked-normal.png`, `blocked-developer.png` and
`blocked-disabled.png` were inspected to confirm the text is absent/present/absent.
`passed.json`, `checkpoints.json` and the full trace preserve the corresponding
state and exact package/binary identity. This is macOS Metal correctness coverage
with ordinary display pacing, not a throughput or cross-platform claim.
The capture waited until the other native elevator validator left a quiet
interval; it did not stop or alter that task.
