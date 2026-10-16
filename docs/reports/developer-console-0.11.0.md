# Developer console — 0.11.0 staging

This stage extends the opt-in native console and addresses issue #22, including
its subsequent `/become` scope. Enable **Settings → Controls → Developer Console**
and press backtick in play. Escape closes it. Typing pauses the world, releases
the mouse and suppresses gameplay input. Each command accepts `help`;
`/help <command>` works too. History and scroll controls retain their existing
bounds. Ordinary play gains no tutorial or throw text.

| Command | Contract |
| --- | --- |
| `/give list` | 58 explicitly reviewed pickup IDs/names, sorted by stable ID; scroll for the complete list. |
| `/give <id>` | Exact allowlist lookup; always dynamic, pickup-enabled and immediately movable. |
| `/respawn` | Validate the standing spawn against map geometry and live item bodies, safely release any held item, then restore only the player. |
| `/pos` | Eye/feet in metres; yaw/pitch in degrees and facing vector. +Y is up, yaw 0 faces −Z and yaw 90 faces +X. |
| `/remove` | Requires and removes the held physical item. |
| `/remove all` | Requires a held item; removes matching canonical catalog identities from session and authored movable physics props. Fixed decorations survive. |
| `/physics pause\|resume\|step` | Pause/resume rigid-body simulation, or immediately advance exactly one normal 1/120-second tick and stay paused. |
| `/time scale <0..4>` | Finite bounded world multiplier: 1 normal, 0 stopped. |
| `/trigger <id>` | Enter a real trigger volume through normal event bindings, conditions, cooldowns and one-shot rules. |
| `/trigger show [on\|off]` | Toggle up to 16 nearby trigger-volume IDs within 12 m and a 60-degree half-cone. |
| `/become list` | Curated pickup representations and an explicit portable-character list. |
| `/become <model>` | Select the visual and enter third person; `party_cup` and `spooner_man` are aliases. |
| `/become place` | Copy only the selected model's current feet/yaw transform into a session prop. |
| `/become phys on\|off` | Configure future copies: default off; on starts dormant until pickup, physical activation or a typed trigger/game-mode activation. |
| `/become reset` | Safely release a held item and restore the normal first-person visual. |

## Reusable ownership and lifetime

`game::gameplay_actions` defines typed requests. `game::gameplay_runtime` applies
them against a `GameplayContext` containing live game, renderer, item-world and
authored-item owners. `Runtime::apply_action` needs no console or command string.
The console parser and output adapter translate requests and present results;
future game modes/trigger adapters can use the same backend, including keyed
`ActivatePhysics` without adding a textual command. No editor, multiplayer,
persistence format, audio stage or caustics stage is introduced.

The [complete catalog audit](console-pickup-audit-0.11.0.md) records all 593
entries, measured eligible bounds, masses/colliders and exact exclusions. The
allowlist and execution share `pickup_catalog::ITEMS/find`; physical coefficients
remain in the existing `assets/gameplay.json` sidecar. Asset eligibility does not
change an authored instance's physics choice. Trees, buildings, fixed scenery,
non-model resources and unreviewed models cannot bypass the allowlist by ID.

At most 16 session props exist across give and representation copies, with a
bounded 256-key allocation lane per developer session. Failed allocation rolls
back physics and visual resources; deletion frees live-prop capacity. Removals
clean bodies/colliders, held/contact state, visuals and authored entity handles,
components, names and listeners. Generated/trimmed authored IDs use the same
resolver as the entity world. Reload restores original authored content; source
JSON and compiled packages are never edited. Map replacement or disabling the
console removes session visuals. Player-only respawn preserves world objects;
generic player teleports detach held control at the item's retained valid pose.
A full authored `reset_to_start` additionally clears session props/representation,
labels and clock controls; a separate generation prevents player-only recovery
from deleting them or erasing commands submitted after the reset.

The existing 16-mesh dynamic renderer cache remains bounded and shared with map
objects. At capacity it replaces only unused asset slots, preserving live
objects and explicitly registered handles. GPU resources and visibility caches
follow replacement. Sequentially giving/removing different models therefore
does not exhaust capacity merely from retained cache history.

## Simulation clocks and collision

Player motion/look, input and held-object geometric control use the unscaled
clock. World physics consumes the multiplier once in a bounded fixed-step
accumulator, retaining a 120 Hz solver and at most 12 catch-up steps per frame.
Excess work is discarded/reported rather than enlarging integration intervals.
Doors, route/AI/timer logic, weather, world animation and light-response timers
use bounded scaled world time. The selected player's animation uses the player
clock. Opening the console or menus still pauses both input-driven updates.

Explicit physics pause overrides a positive time scale for rigid bodies only;
other world systems retain their configured world clock. Resume respects scale
zero. Single-step discards the fractional accumulator and publishes exactly one
fixed tick immediately, even while typing or at scale zero, then stays paused.
Held control remains swept and collision constrained without a solver tick.
Release/throw retains the valid release pose and bounded real-time velocity,
without inverse scaling or division by zero. At zero/pause the released object
stays frozen until world physics advances. Player sweeps also constrain stopped
item bodies, including empty-handed walking, falls and head contacts.

Noclip exit retains a clear airborne eye/yaw/pitch and resumes gravity. Exiting
inside geometry uses a revalidated entry/spawn or refuses safely. Genuine void
recovery retains its ordinary grace path rather than floor-snapping an otherwise
clear airborne pose.
The current, remembered and spawn candidates also pass live physical-item
capsule clearance. Console disabling follows the same typed exit policy; a
rejected exit retains noclip and its current pose.

## Representation and rendering

Player control, collision capsule, identity and interaction eye are unchanged.
Visual models keep their authored metre scale unless their largest extent exceeds
2 m, when they are uniformly reduced. Measured bounds anchor the bottom and
horizontal centre to player feet; +Z model fronts map correctly to player yaw.
The camera boom adapts to visual size and sweeps a sphere against static
geometry, floor/ceiling, live doors and physics items. Game aiming remains at the
player eye. A very small model therefore has a low close camera while its player
collision body retains normal human dimensions.

Rigid and rigged/clip-only models use existing cached renderer paths. Runtime
character transforms support full quaternion physics orientation independently
of animation. Physical copies use validated dormant placement and ordinary
activation/recovery. Interact activates a dormant copy that is too large to
carry, through the same occlusion-checked physical target path; smaller copies
can be picked up normally. Physics-off copies are stationary visual props with no
player-control or physics body. Selection is transactional: failed model/resource
validation or obstructed release retains the old representation and held item.
Every dormant activation path checks the full player capsule before changing
body type. A copy still enclosing the player stays frozen until the player moves
clear; ordinary surface contact does not prevent activation.

The lit cake retains all seven embedded flame nodes, `candle_flicker`, emissive
material and the reference attached warm glow through a shared `GlowCue` profile.
The bounded light budget rejects an unallocatable copy and rolls it back. Light
keys disappear with their owning character; suspended physics characters hide
both their geometry and glow. Trigger labels reuse interaction cone/LOS queries
and the flashlight's shared prepared triangle transmittance, including floor,
ceiling and moving occluders, independently of flashlight power.
Displayed IDs share the command parser's 128-byte logical-ID limit. Their exact
text scales to fit and occupies separate, edge-clamped screen rows when nearby
anchors overlap; at most sixteen labels and eighteen layout rows are considered.

## Verification

The seven essential CPU regressions use empty/minimal in-memory worlds, at most two
unit cubes and no GPU, asset loading or map bake. They protect exact-tick/remainder
isolation and numerical scale boundaries, frozen carry/release, empty-hand frozen
player collision, reset detachment, generated-ID authored deletion/reset, the
original airborne noclip bug and dormant activation around the player. They live
in owning `engine/tests` and `game/tests`
folders with actual module wiring; the broad deleted suite is not restored.
Native acceptance remains explicit developer tooling outside Cargo/CI.
The activation fixture uses one cube and narrow-phase queries without a solver tick.

October 16 final gates passed:

- `cargo fmt --all --check` and `git diff --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`, plus
  the locked release Clippy gate.
- `cargo test --workspace`: seven tests, no failures, under 0.01 s of test work.
- `cargo build --locked --release --workspace --all-features`: all four
  production executables, 2m20s on this machine.
- `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'`:
  372 discovered, four skipped, OK in 103.8 s. No map bake was required.
- Catalog validation: 593 entries, 370 placeables, zero warnings. Prop generator
  check passed. Texture check passed for 123 textures with 96 existing manifest
  coverage warnings; no texture imagery changed in this stage.

The final direct executable's SHA-256 is
`c3b080fce2eeb37f3c0a19f0770bd57424a7023e7ae09bb413a83fb00a35fb83`.
Every accepted session below used that same executable on Apple M2 Pro / Metal.
The independently compiled small fixture's SHA-256 is
`47e7dcd9de740e59f495d64c942cbadee6fa5bbdb37aba199a194871bdba7883`;
the final compiler's `verify --require-current` reports no differences.

| Explicit native campaign | Passed assertions | Main evidence |
| --- | ---: | --- |
| Controls | 73 | Help, invalid input, actual E pickup/removal, exact/generated authored identities, fixed cup preservation, triggers/visibility, copies, full reset and real reload. |
| Models | 58 | Lit cake/flames/glow, pickup/drop/throw at zero, 90-degree rigid orientations, contact, camera obstruction, physical Spooner-Man, large rig activation refusal and ordinary movement out before activation. |
| Clock | 82 | Ordinary 500 ms W input moves about 1.5 m at 0, 0.25, 1, 2, 4 and paused 4; the held duck follows, released bodies freeze, exact stepping and pause/resume precedence remain correct. |
| Complete pickup catalog, four chunks | 620 | All 58 IDs successfully create one active movable body and exact session identity; each disable cleans physics, geometry, character and session resources before the next model. |
| Recovery edge cases | 15 | Exact airborne pose retention followed by natural landing over clear floor/ramp/stairs/near-wall positions; solid ceiling and physical-prop fallback; genuine void exit retains pose before delayed recovery. |
| Existing Office demo | 10 | Cup and Spooner-Man representations, movement/animation at world scale zero, animated physics-off duplicate, first-person reset and safe respawn. |
| **Total** | **858** | All native processes exited successfully. |

Accepted native sessions used desktop fullscreen, actual logical 1512×949 /
drawable **3024×1898**, with no missed ready presentations. Earlier occluded
windowed runs and diagnostic placement/pose failures are retained separately and
excluded. The native campaign found and corrected the retained-mesh capacity
leak, overlapping trigger text, and the live-body noclip exit case. Independent
state/resource review has no outstanding findings.

The capture-free Office timing run used High quality/filtering, Full lightmaps
and reflections, bloom on, V-sync off, no frame limiter and actual **Immediate**
presentation. After 240 warmup frames, 3,600 measured frames averaged **195.1 FPS**
(mean loop 5.126 ms, median swap 0.045 ms). All 3,840 ready frames presented.
The active built-in display reports 120 Hz; throughput exceeded it and did not
plateau at refresh, so this sample is uncapped. This is one view on one machine,
with no before/after performance improvement claimed. Capture sessions are not
used for FPS comparisons.

Preserved logs, direct executables, packages, action scripts, raw captures and
`verification.json` live outside `target` in `local/developer-console-0.11.0/`.
Curated full-frame native images:

- [Readable local trigger IDs](../images/developer-console-0.11.0/trigger-labels.png).
- [Held lit cake](../images/developer-console-0.11.0/cake-held.png).
- [Physical Spooner-Man at a 90-degree orientation](../images/developer-console-0.11.0/spooner-tilted.png).
- [Third-person camera near a wall](../images/developer-console-0.11.0/spooner-wall-camera.png).
- [Cup representation in Office](../images/developer-console-0.11.0/office-cup.png).
- [Spooner-Man walking in Office at stopped world time](../images/developer-console-0.11.0/office-cat-walking.png).

To repeat the maintained native campaign, explicitly build the small fixture
with `places-compile build tests/fixtures/native/developer-console.level.json
--out <fixture.placesmap> --asset-root assets`, then run
`tools/bench/console_stage.py --binary <direct-executable> --map
<fixture.placesmap> --fullscreen --mode controls` (also `clock`, `models`, and
`catalog --chunk 0` through `3`). These operations remain outside ordinary CI.

Limits: native acceptance covers macOS/Metal. Copies are session-local, player
collision remains the normal human capsule, and characters without a dedicated
physics profile use a validated coarse bounds collider and 20 kg default mass.
No anatomy-specific collision, multiplayer, editor, audio or caustics work is
included. Exact-commit CI and final branch/issue closure are recorded on
[issue #22](https://github.com/csd113/Places/issues/22).
