# Game Modes foundation — 0.14.0

The native editor now has **Project > Game Modes** for a selected map's health,
regeneration, health display, movement speed and crowbar rules. New walking modes
start with health off, no tool and 3.75 m/s walking. Existing maps without a mode
keep their earlier health/tool defaults and global movement setting. Explicit
mode speeds override that setting during grounded, crouched, water and ladder
movement; developer noclip retains its separate settings behavior.

A mode has a name and bounded speed, with the existing validated health and tool
values. Applying a preset copies these values in one undo step. Map saves publish
an immutable, content-addressed JSON copy under project-local `game_modes/`
before atomically replacing the level source. The level's self-contained rules
must agree with that copy on reopen/build. Missing, conflicting or symlinked
copies are refused. Earlier snapshots are retained locally; only currently
referenced snapshots enter a complete project export.

Presets are separate files in the personal Places user-data `game_modes/` folder.
The library admits up to 128 bounded, versioned presets. Naming a mode and saving
creates a preset; saving the same name explicitly updates it with a disk-conflict
check. Changing a preset never changes an already applied map. Rename first to
save another preset. Library writes are separate from map undo/save operations.

Compiled maps carry the complete rules and require `game-modes-v1`. Player
startup never opens a loose mode file or consults the preset library. Complete
project validation compares runtime rules, editable rules and local copies;
source recovery preserves the selected mode files.

## Verification

Evidence belongs to `local/verification/game-modes-20261111/`, outside the build
cache. The pre-task diff and hashes record the already approved movement/assets;
this task adds only the speed field/getter to their shared entity-runtime file.
No earlier work is discarded or staged independently.

Two bounded CPU regressions protect preset-copy independence, ordinary history,
reopen/save conflict refusal, package source inclusion, actual movement override,
legacy defaults and level-reset isolation. They use tiny source/flat-world
fixtures without baking maps or loading assets. The final workspace test run
passed all 593 tests; strict all-workspace/all-target/all-feature Clippy and
`cargo build --locked --release --workspace --all-features` also passed.
Formatting and diff whitespace checks passed.

## Scope limits

This foundation does not add a dedicated mode import/export UI, mode-specific
Luau rules, arbitrary modifiers or zombie waves. Existing general map scripting
remains separate. Crowbar/HUD artwork and NPC movement are unchanged from the
previous approved phase. Windows validation remains deferred.


## Native results

Five development-editor processes exercised actual visible controls: create and
save a named preset, apply it to a second map, update the first map/library,
reopen the second map, and open/save/play an untouched legacy map. The first map
and updated library used 2 m/s with health off; the second retained 6 m/s with
health and regeneration enabled. The legacy source acquired no mode, health or
tool fields. The recovered project contains six byte-identical source files:
manifest, three levels and two selected mode snapshots. A deliberately conflicting
mode copy was rejected by the compiler rather than silently accepted.

The final production editor reopened the recovered second map with an empty
personal preset library, saved successfully and entered embedded play with the
copied rules. Its Game Modes, health and movement screenshots were inspected.
The final production player imported the complete project into three isolated
profiles, one for each map. The enabled map dropped from 6 HP to 2 HP after a
diagnostic damage action, then regenerated to 6 HP using the authored delay/rate.
Neither disabled map had active player-health state. The legacy map had no tool;
the two explicit modes retained 3 HP damage, 2 m reach and 2.5 swings/second.
With a 2 m/s global preference, the 6 m/s mode moved substantially farther than
the two 2 m/s cases under the same native input recipe. Exact settled movement
speed and reset isolation are covered by the CPU regression; timed native input
and screenshots are behavior/presentation evidence, not FPS measurements.

All native runs exited successfully. Evidence includes recipes, receipts, logs,
framebuffer captures, source identities and package recovery comparisons.
`production-editor/game-modes.png`, `production-editor/health.png` and
`production-editor/movement.png` show the final production UI. The three final
production executables are copied and hash-verified under `preserved-build/`.
No commit, push, main-branch change or release was performed for this phase.
