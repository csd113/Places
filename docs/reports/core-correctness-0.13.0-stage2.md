# Places 0.13.0 stage 2: core correctness

Completed on the connected Mac on 2026-10-20, continuing `0.13.0-staging` from
`f70dbd1b560a1ea759cd22fc76c7154a1bb72035`. Local HEAD and the live remote were
verified before editing and again before delivery. The previous stage was idle;
no competing game, compiler or Cargo writer was present. The unrelated Python
audit/inventory reports and all stage 1 evidence remain untouched.

The current #27/#28 descriptions, repository instructions, canonical map guide,
`baseline-0.13.0-stage1.md`, checkpoint evidence, 15 native receipts and relevant
captures informed this work. The baseline matrix was not repeated. Raw stage 2
receipts, action scripts, captures, logs, summaries and direct binaries remain
outside `target` under `local/stage2-0.13.0/` on this Mac.

## Causes and changes

**Airborne ramp collision.** Horizontal terrain sweeps closed ramp perimeters,
but omitted the interior contour where the rising slope crosses the feet plane.
The airborne height query discarded the now-higher ramp and treated that space
as unsupported. Once inside the wedge, descent also discarded the surface above
the previous feet, allowing a fall through to the void. A tiny elevated-ramp
controller fixture reproduced this before the fix. The shared floor model now
emits that airborne contour and lands on the highest real slope point within the
circular feet footprint, using the circle's chord at side edges. Walking retains
its existing step rule. No map coordinates, invisible replacement floor, larger
recovery envelope or respawn workaround were added to the controller.

The reported stair failure remains less specific: neither stage 1's ascent nor
this stage's ascent, jumping descent and crouched descent reproduced it. Existing
riser/perimeter handling is retained; a small tread/side-edge landing regression
protects it while the shared terrain path changes. This is evidence for the
exercised paths, not proof that every reported stair trajectory is fixed.

**Dormant movable props.** The player controller stopped before dormant shapes,
while activation only considered explicit actions and active rigid-body impacts.
An all-dormant world then skipped solving entirely. Before that fast path, the
physics world now recognizes the player's exact shape contact, including its
small swept contact gap, and activates configured movable bodies. Fixed bodies
stay fixed. Pause/zero scale, disabled player collision, suspended bodies and the
existing player-penetration refusal remain enforced. Ordinary solver sleep after
activation remains distinct from authored dormancy. The established rule that
movable rigid props are pushed rather than controller-owned standable floors is
unchanged.

**NPC animation ownership.** Unaddressed placed characters explicitly inherited
the player's locomotion snapshot. That input is removed from the shared scene
update API. Each actor follows its own action cue, explicit locomotion input or
frame displacement; stationary skinned actors advance idle. Rigid animated props
retain their pose without an action. The player's visual representation still
receives its own simulation snapshot separately. Authored poses and intentional
stationary walk cues retain precedence.

**Required supply recovery.** Intake had a player-only well catch and whole-visit
reset, with no item-loss policy. The required box now has its own
`essential_supply` reset group. An exact-instance, unheld-item volume in the well
uses the existing atomic `reset_physics_group` action. The accessible well sign
also requests that group, providing recovery from other inaccessible placements
and retry after clearing an occupied return point. It preserves other objects
and objective state, creates no duplicate and refuses unsafe placement. After
acceptance, the sign preserves the delivered box; the existing delivery guard
prevents repeated rewards. Player falls still use the established visit reset.
This is authored policy using shared capabilities, with no per-map engine branch
or new editor/schema subsystem. Guidance and the authoring guide document it.

**Release and feedback.** `can_drop` treated every shape intersection as unsafe,
including a harmless resting overlap of ten microns. The focused fixture failed
before the change. Intersecting shapes now receive a distance check against the
existing `CONTACT_EPS` (0.1 mm); genuine penetration and failed shape queries still
refuse release and retain ownership. Camera clearance and swept carrying remain.
The ordinary HUD uses short feedback, “Make room to set this down.” or “The item
cannot follow here.” Raw diagnostic labels and the unbounded authored-item ID are
removed from that surface. Detailed body state and blocked-action events remain
in developer traces/authoring diagnostics. This fixes a demonstrated false
refusal, without claiming that every historical warning had that cause.

## Native validation

Direct preserved executable:
`local/stage2-0.13.0/source-binaries/places`, SHA-256
`7c70e8f76d4475de6fcdc0bb6e3def5e8c080d33f54e390554b2b8b025bd4497`.
MacBook Pro Mac14,9 / M2 Pro, macOS 27.0.1, Metal, High textures/filtering/lighting,
Full lightmaps/reflections, bloom, 60-degree FOV. All cases used V-Sync off and no
frame limiter, **640×360 logical / 1280×720 actual drawable**, actual Immediate
presentation and zero missed ready surfaces. Gameplay used real delta. The ramp
unit fixture additionally covers 30 Hz, 144 Hz and mixed intervals on positive
and negative slopes along both axes. Diagnostic captures and interactions are
correctness evidence; no FPS improvement or matched performance comparison is
claimed. No temporary app wrapper was used.

Starting placements and fixture manipulations below are explicit diagnostics;
walking, jumping, pickup, drop, throw and door input use the production paths.
`native-summary.json` provides compact selected events; per-case environments,
receipts and action scripts preserve the exact setup.

| Evidence directory | Result |
| --- | --- |
| `tower-ramp-r1` | Saved far-ramp start eye (16.75,8,1.65), forward and four jumps: reaches eye (16.75,11.2,9.70), feet 9.6, grounded, reset count 0. Stage 1's corresponding path fell and reset once. |
| `tower-negative-ramp-r1` | Middle negative-rise ramp: jumping ascent reaches feet 6.4 at the north landing; descent reaches feet 3.2 at the south landing, reset count 0. A separate side setup was displaced by spawn clearance near an adjoining guardrail, so it does not prove the intended side traversal. |
| `office-ramp-r2` | Correctly elevated east-shaft control: repeated-jump ascent reaches feet 7.2 and descent reaches feet 3.6, grounded with reset count 0. Existing environment package and geometry unchanged. |
| `tower-stairs-r1` | Repeated-jump ascent reaches feet 3.2; jumping and crouched descent return to feet 0, reset count 0. Outside-foot side approach is blocked safely; it does not establish an ascent along the full outer edge. |
| `pool-contact-r1` | Wet-floor sign contact changes 19 dormant bodies to 18 dormant plus one active, then sleeping. Player passes to z11.99 and backs to z13.47, reset count 0. Actual initial eye z15.55 after clearance; this is not a matched timing comparison with stage 1's z15.4 setup. |
| `tower-npc-r1`, `home-npc-r1` | Technician remains `pose_stand` at (3,0,4) during player lateral walking/stopping. Upper technician and witness retain their independent authored cues. Home caretaker moves from x5.5 to x4.55 via its navigation goal, using walk then standing, while player eye stays (5.5,1.6,6). Captures visually inspected. |
| `well-delivery-r2` | Two ordinary E pickup/drop losses into the well return the same box to (10,0,10). Ordinary delivery settles it at approximately (3.957,0.002,7.977), emits one acceptance and unlocks the airlock. Post-completion sign leaves it sleeping there. Unrelated spray bottle stays at its authored pose. |
| `recovery-retry-r1` | Occupying the return point with another prop causes automatic and manual reset refusal with identical before/after body state. Moving that blocker away and using the sign returns the box. Delivery succeeds, the ordinary door/airlock path enters Tower with empty hands, and a diagnostic reload returns to fresh Intake with exactly one required supply. |
| `release-r1`, `home-carry-r1` | A diagnostic overlapping prop in paused Pool produces a real blocked drop and retained ownership; capture shows short feedback. Clearing it permits retry, pickup and throw, then normal settling on resume. Home ordinary cushion pickup/drop/pickup/throw also succeeds. |
| `long-error-r2` | Tiny unsafe-path ZIP import is rejected by existing validation. Its long error remains within two fitted on-screen status rows; native capture visually inspected. |

The three-map Relay export is retained as
`local/stage2-0.13.0/Relay-Annex-Stage2.placesmap`, 24,070,169 bytes, SHA-256
`8c3ae62322a515d5208163f4c793341544ffa7180e3425bb770e5106bf5acbd1`.
The production compiler validated/exported the changed gameplay sources. Its
prepared-cache checks missed for all three members, so that project was prepared
once; no environment-collection rebake or geometry/art edit was performed.
Original packages remain intact. The compiler executable is preserved beside
the player, with its hash and command in the local checkpoint/export log.

## Checks and review

All required checks exit 0:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`: **198 passed**, no full-map bake; suite execution about 0.4 seconds.
- `cargo build --locked --release --workspace --all-features`
- `python3 tools/assets/validate.py`
- `python3 tools/textures/build.py --check`
- `python3 tools/props/build.py --check`
- `git diff --check`

Six focused tests add concrete protection for airborne ramp entry, stair-edge
landing, first dormant contact/fixed policy, resting release/real-overlap
refusal, actor animation ownership, and the actual Intake script's recovery and
single-reward policy. Fixtures are tiny metadata worlds or one synthetic rig,
with no renderer, texture generation, source-map baking or broad suite
restoration. Test-only interval expansion did not change the preserved production
binaries. Final logs are `*-final.log`; red/green reproduction logs remain.
The catalog validator reports zero warnings. Texture checking retains 96 existing
preferred-resolution/manifest advisories, and prop checking retains existing art
budget annotations; no visual asset was changed to silence them.

Review checked the shared ownership boundaries, tolerances, fixed-body exclusion,
pause and penetration guards, atomic reset refusal, quest identity/reward guards,
player representation, ordinary HUD and scoped source diff. No blanket lint
suppression, format/cap increase or unrelated cleanup was introduced.

## Remaining limits and next-stage custody

- The original unspecified stair fall-through remains unconfirmed beyond the
  paths above. Keep its report and stage 1 evidence; successful attempts are not
  universal proof. Broader stair/ramp authoring/export validation belongs to the
  editor stage unless a new concrete runtime reproduction identifies a defect.
- Severe Home cushion/books-on-bed and Pool closet slowdowns remain for the full
  performance stage. These correctness runs do not establish a performance fix.
- The Relay ceiling artifact is still visible in retained evidence; no general
  ceiling, editor or geometry rewrite was attempted.
- Essential-item loss handling is explicit authored policy. The well auto-returns
  this box; the sign supplies manual recovery for other inaccessible placements.
  This is not an automatic reachability solver for arbitrary pits/water/puzzles.
  Persistent campaign saves are unsupported by the existing game; section
  transition/reload intentionally starts fresh local state and was checked.
- Setup failures are retained honestly: `well-recovery-r1` released before the
  distant swept hand arrived, and missed delivery; `long-error-r1` selected a map
  instead of Import; `office-ramp-r1` started beneath the east shaft's actual
  floor. They are not passing evidence for those attempted checks.
- Water implementation, storage/compiler optimization, full performance, audio,
  UI/menu redesign and visual styles remain later stages. Cloud model rebuilding
  stays paused; Spoonerman and Windows/Linux environment work remain excluded.

Stage 2 stops here. Final integrated CI waiting remains with the final queue
stage; this handoff does not merge main or publish a release.
