# Backrooms encounter foundation — 0.14.0

The source project is `projects/encounter-lab/`. Its single 32 × 32 m room has
a 10.5 m ceiling, the existing Backrooms materials and exactly two corner spawn
zones. Three existing wall-switch models provide Start zombies, Clear zombies
and Toggle companion interactions. Aim and press E. The start subtitle is
“zombies incoming”. There are no waves or spectating.

Each zone owns ten spawns and a separate one-second simulation clock. Repeated
start requests cannot restart the clock or add another cohort. Clear removes
living zombies and their bodies, resets both clocks and permits another start.
The companion toggle is independent. Death retains the first-person physical
camera and offers E to respawn; respawn removes zombies, bodies and the robot,
restores the player and waits for another start.

Health remains disabled by default. This project explicitly enables it, with
regeneration off and configurable 0.35-second hit protection. A zone alone does
not enable health or run an encounter. Old maps retain their existing behavior.

## Combat and ownership

The existing zombie and robot atlases are reused. Sixteen zombies per cohort
move at 90% of the configured player walking speed; four move at 110%, with the
allocation changing between rounds. Each zombie has four HP; the map's two-HP
crowbar therefore kills in two accepted hits. Zombies choose between the player
and living robot. The robot first selects zombies pursuing it, then the nearest
other zombie, and otherwise follows the player.

Movement uses the existing navigation and world collision. A swing has one
contact at 32% of its configured interval, with range, original target and
wall/door visibility checked again at contact. The robot uses the map's crowbar
damage, reach and speed. Its existing crowbar model follows the actual right-hand
socket and disappears on death or removal.

Death hands the current mannequin pose to articulated physics once and retires
the actor's navigation. Corpses persist until clear/reset. Crowbar contact casts
against actual ragdoll colliders, applies an impulse at the struck bone and wakes
the body. Ragdoll bones never enter the pickup registry. The shared budget now
supports 24 ragdolls, covering twenty zombies, robot and player within the
existing 32-bit collision-group layout. Clear releases solver bodies, joints,
colliders, posed-bone caches, entity components, AI and render ownership.

## Editor and assets

Select a Spawn point, open Advanced > Object settings, and enable Spawn zone.
Count, Cooldown s, Width m and Depth m are independent per point. Normal movement,
rotation, history, validation and save/load apply. Enabling a zone removes the
ordinary point's exclusive spawn group. The initial controller is explicit map
source data; its contract and bounded limits are in `MAP_AUTHORING_GUIDE.md`.

Five clips were appended to the accepted mannequin: crouch idle/walk and crowbar
idle/walk/swing. All 19 original clips, mesh, rig, UVs, materials, embedded PNGs,
existing accessors and original binary payload were preserved. The offline
exporter verifies that preservation before writing. No texture imagery was
generated. The player remains hidden in ordinary first person; the external
diagnostic camera was used to inspect crouching and standing again.

## Validation

Strict workspace Clippy with all targets/features and `-D warnings`, the locked
release workspace build with all features, formatting and all 588 Rust tests
passed. Four narrow regressions add unique coverage for independent clocks and
repeated cleanup, companion target priority and single-contact attacks, physical
corpse contact without pickup, and independent editor zone tuning/history/save.
They require no map baking. The encounter fixture is tiny in-memory data; the
physics check uses the existing mannequin fixture and one solver step.

The explicit asset sweep sampled 31 poses in each new clip and reported no
problems. Clip-boundary checks passed for all shipped models. The native package
build produced no warnings; integrity and `verify --require-current` passed.

Native Mac/Metal evidence is under the ignored directory
`target/verification/encounter-20261110/`. The room uses the Static package variant
(runtime name `off`), at a 1920 × 1080 physical drawable. These are correctness
and memory checks, not FPS comparisons.

- Real E interaction started the authored encounter and displayed its subtitle.
  The robot's skin, attached crowbar, attacks and eventual death were observed;
  five zombies died through companion combat in the smoke run.
- Real mouse inputs left a zombie alive after one contact and created its
  ragdoll after the second. Native traces identify both contacts against the
  same zombie. Earlier attempts to strike its corpse were correctly occluded
  by living zombies standing over it; those are not counted as corpse-hit proof.
- Native external-camera captures inspected crouch, crouch walk and return to
  standing. The append-only asset sweep separately checked contact/stretch and
  loop boundaries.
- The native editor changed the first zone's cooldown to 1.5 seconds, retained
  the second at 1 second, saved successfully and reopened those values in an
  isolated project copy.
  The actual test project retains 1 second for both zones.

The isolated corpse run drew living zombies away before returning to the body.
The trace then recorded `Crowbar hit ragdoll encounter:zombie:0`. The settled
body went from zero measured speed to 1.83 m/s, with its upper-body joint moving
about 14 cm between captures. E left the held-item owner empty. This distinguishes
an actual articulated hit from a living actor occluding the corpse.

The complete `campaign-4` run lasted 223.7 seconds in **one game process**:

- Ten full cohorts: 200 zombies, twenty per round, always sixteen slow/four fast.
- Twenty persistent physical corpses per full round, followed by clear.
- Ten additional mid-spawn clears and repeated start requests during each round.
- 86 companion-toggle requests, including the initial physics warmup and a
  rapid 64-toggle sequence; one E respawn and a successful subsequent start.
- Every clear returned to the same 22 entities, 48 component entries, three
  bindings, four rendered characters (player and three controls), zero dynamic
  props, zero spawned AI and zero ragdolls. Physics returned to one player body
  and seven colliders, with zero joints or posed-bone caches. The lazy static
  physics world was initialized before taking this baseline.

Cleared-state RSS in MiB by full round was 348.14, 350.11, 351.12, 351.20, 329.41,
329.52, 329.72, 331.94, 332.02 and 335.34. Excluding three warmup rounds gives
329.41–351.20 MiB and a −15.86 MiB first-to-last change. These samples show no
sustained upward trend in this run, but include normal allocator/OS variation.
Retained benchmark-frame storage was zero throughout. Across the twenty-body
checkpoints, maximum joint-anchor error was 7.18 mm and maximum reported contact
penetration was 6.06 mm; those are sampled checkpoints, not continuous maxima.

The reusable explicit tool is `tools/encounter/check_native.py`. It is outside
Cargo/CI, launches a visible native process, compares actual ownership and RSS,
and reports its action counts from the executed script. Lethal health actions
accelerate the twenty-corpse cohorts; player aim and robot combat were tested
separately. The final script also runs four consecutive death/respawn cycles;
`--rounds 0` runs that reset/toggle portion alone.

A separate four-respawn session passed, but its first-to-last RSS rose 6 MiB.
That prompted the longer `respawn-16-complete` check: **sixteen consecutive
death/E-respawn/restart cycles in one process**, lasting 122.2 seconds, with
82 companion-toggle requests. All sixteen restored full player health, cleared
the encounter, returned every tracked ownership count to baseline and spawned
the first pair again after Start. RSS initially warmed from 346.44 to 361.73 MiB,
remained 361.89–361.97 MiB for four more resets, then fell and finished at
339.94 MiB. Post-warmup samples ranged 336.33–361.97 MiB, ending 12.41 MiB below
the fourth-reset sample. No persistent upward trend or growing owned-resource
count was observed. This run used `--rounds 0 --respawns 16`; the ten full cohorts
and these sixteen resets were separate native sessions, not one combined run.

Earlier action-file-size/delay rejections occurred before game startup. A separate
run lost its trace when the host disk filled, so its incomplete evidence was
discarded from the memory conclusion. The successful run above recorded through
normal shutdown after space became available. No unrelated files were removed.

## Limits

This is a bounded first encounter controller, not a general Game Modes system.
It supports at most twenty zone actors and one companion. Navigation avoids
world geometry; sophisticated crowd separation, formation tactics and polished
combat balance are not implemented. Living-character tool contacts retain the
existing posed bounding boxes.

Encounter checkpoint persistence is not implemented, so encounter maps require
`saving: false`. The editor can author the zones, but embedded Quick test retains
its existing NPC-ragdoll limitation; use a compiled package for full combat.
Windows validation was deliberately deferred. A short native campaign cannot
prove the absence of every possible memory leak.
