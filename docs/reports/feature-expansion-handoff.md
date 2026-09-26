# Feature-expansion handoff — Runs 01–05

This report is the running record for the numbered feature-expansion runs. It
holds implemented behaviour, actual paths and interfaces, commands and results,
known issues and next-run dependencies. It is not architecture documentation:
durable design belongs in `docs/ARCHITECTURE.md`, `docs/RENDERER.md` and
`docs/MAP_AUTHORING_GUIDE.md`.

## 1. Repository baseline (before Run 01 edits)

- Branch `main`, working tree clean at `8bd9b4e` (“lighting upgrade”); the
  preceding movement/water work is commit `7f05d00`.
- `cargo test --workspace --no-fail-fast`: **1138 passed, 0 failed, 9 ignored**
  (425 s, includes the lightmap bakes). This is the run-0 baseline; nothing in
  this run may regress it.
- Controller baseline (`src/game.rs`):
  - `GRAVITY = 18.0` (not the requested 9.8), `JUMP_APEX_M = 0.75`,
    `JUMP_VELOCITY = 5.196_152`, fixed vertical substep `1/120 s`, at most 12
    substeps per frame, `MAX_SIM_DELTA = 0.1`.
  - Vertical support is the walkable floor only; landing uses
    `walk_height_at(...).unwrap_or(player_floor_y)`, i.e. the last floor is an
    invisible floor off-room, and there is no prop/box support or box-underside
    collision. Falling through a doorway header while jumping can re-enter the
    header’s horizontal band (`WallAabb::intersects_player_y`) and
    `resolve_player_collision`’s centre-inside branch snaps the player to the
    nearest face (the forward teleport).
  - Walking refuses any drop larger than `PLAYER_STEP_HEIGHT = 0.4`; walking
    off ledges, into the demo pool or into The Pit’s carpet holes is impossible.
  - `PLAYER_HEIGHT = 1.8`, `EYE_HEIGHT = 1.6` are constants; no crouch/stance
    exists anywhere in `src/` and no key binding is reserved.
  - Water: `WaterVolumes` + `swim_vertical`; the swim eye is a separate
    `FLOAT_EYE_MARGIN` above the surface; bottom rest is
    `floor + SWIM_FLOOR_CLEARANCE`. `leave_water` maps the swim eye straight to
    `support + EYE_HEIGHT` whenever the virtual feet are within a step of a
    floor, which can pop the player up where the water ends.
  - Ladders: **none**. `core:pool_ladder` at `assets/levels/places_demo.json`
    line 1513 (x 19.72, z 12.0, rot 90°, `solid: true`) is an obstacle with an
    axis-aligned collision box x 19.495..19.945, y −3.0..−0.8,
    z 11.725..12.275. That box blocks the water side and the deck side of the
    rails; the only authored pool exit is the walk-in step.
- Level data: canonical demo `assets/levels/places_demo.json`; Pit level
  `levels/level0_pit.json`; Home stairs/ramp fixture
  `tests/fixtures/levels/home_showcase.json`. The demo pool is basin region
  x 8..20, z 10..16 at −3.0 under water surface −1.65 (deck −1.5); the office
  desk is `core:desk`, top exactly 0.75 m above its floor.
- Checks to run for this run: `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo test --workspace`, plus the focused
  `game`/`collision`/`settings`/`input` filters while iterating.

## 2. Requirement checklist (accumulating)

Legend: [x] implemented and covered by a test, [~] implemented with a caveat,
[ ] not done.

### Falling and jumping

- [x] World unit = 1 m documented; `GRAVITY = 9.8 m/s^2`, frame-rate-stable
      integration with bounded substeps.
- [x] Walking/jumping off an edge loses support and falls (pools, carpet holes,
      ledges); no snap across holes, no clamp to the last floor.
- [x] Solid prop tops are landable and jumpable with blocking sides and
      undersides; the desk-height jump stays usable with launch velocity derived
      from gravity and the measured 0.75 m desk top; step height unchanged.
- [x] Doorway jump forward teleport fixed by real head collision and swept
      movement, with no large depenetration or position reset.
- [x] Head collision under ceilings, frames and props cancels blocked upward
      motion without clipping or pushing through openings.
- [~] Stair entry/exit handled with bounded stepping, headroom checks and
      grounded-only support snapping; no camera smoothing masking collision.
      Stairs carry no soffit colliders (see Remaining issues).

### Water and pool exit

- [x] Stance-dependent body/eye dimensions consistent in water; buoyancy and
      freeboard separate from physical height; no shrinking or double offsets;
      standing on the pool bottom and floating both correct.
- [x] Hold-jump-to-surface preserved with stable surface movement and coherent
      land/water transitions.
- [x] Places Demo pool ladder placement, geometry, submerged reach, deck
      attachment, climb volume and safe top landing repaired.
- [x] Walking into the climbable face raises the player without E; movement
      stays collision-checked; transition onto the real deck without teleporting
      through the rim.
- [x] Release, backing away, jumping, obstruction and water-to-ladder-to-land
      behaviours defined; wrong-side contact never auto-pulls upward; other
      exits stay within jump/step capability; no universal wall mantling.

### Crouch and input

- [x] `C` toggles crouch; pressing it again requests standing; crouched body
      height is exactly half standing height with a valid collision shape and
      eye position; feet anchored during stance changes.
- [x] Standing is clearance-checked: remain crouched when blocked; never move
      through geometry to stand.
- [x] Stance changes on stairs, near edges, in the air, in water and on ladders
      produce no boosts, support snaps or invalid collider dimensions.
- [x] `Crouch` added beside existing movement bindings; rebinding persists via
      the existing settings mechanism and applies consistently; old settings
      files default to `C`; input focus, pause, key repeat and conflict rules
      respected; no sprinting.

### Acceptance

- [x] Focused regressions for ledge falls, prop-top landings, doorway jumps,
      repeated low-ceiling jumps, stair joins, blocked uncrouching, water
      height, ladder exits and input persistence.
- [x] Several frame rates and a simulated hitch exercised.
- [x] Actual demo pool and Home stairs exercised, not only synthetic fixtures.
- [~] Before/after evidence and any unavailable runtime checks recorded.
- [x] Reusable collision/water/ladder interfaces documented for later runs.

## 3. Run 01 implementation

### Units, gravity and jumping (`src/game.rs`)

- One world unit is one metre (unchanged); `GRAVITY = 9.8` m/s^2 replaces the
  placeholder 18.0. Vertical motion keeps the fixed `VERTICAL_SUBSTEP = 1/120`
  integration and the 12-substep bound (`MAX_SIM_DELTA = 0.1`).
- The office desk (`core:desk`, `assets/levels/places_demo.json`) is 0.75 m tall
  by its authored `size` and proxy. `OFFICE_DESK_TOP_M = 0.75`,
  `JUMP_CLEARANCE_M = 0.10`, `JUMP_APEX_M = 0.85`, `JUMP_VELOCITY =
  4.081_666_5` (`sqrt(2 g h)`); `PLAYER_STEP_HEIGHT` stays 0.4.
- `Game::new` / `Game::reset_level` now take a single `CollisionWorld` bundle
  (`game::CollisionWorld`) built by `CollisionWorld::from_level`; the fields stay
  public (`game.walls`, `.floor`, `.water`, `.ceiling`, `.ladders`) and
  `Game::collision_world()` returns the bundle for `PLACES_SPAWN` reloads.

### Falling, prop tops and head collision (`src/game.rs`, `src/collision.rs`)

- `HorizontalMode::Walk` refuses rises over `PLAYER_STEP_HEIGHT` but accepts a
  drop of any size; `StepOutcome` (in `game.rs`) reports `dropped`, the sweep
  marks support lost and `integrate_vertical_substep` starts the fall from the
  eye line without snapping.
- Landing (`support_at`) resolves the highest support under the *centre* whose
  top was not above the feet at the substep's start: the rendered walkable
  floor (`height_at`, so stairs land on the tread underfoot), a solid prop or
  wall top (`highest_support_top`), or the historical world floor at `y = 0`
  outside every room. The last known floor is no longer support.
- Head collision: `Game::head_limit` combines `WalkableCeiling` with
  `lowest_underside` (door headers, window frames, prop undersides) and clamps
  the upward substep, zeroing the velocity; `WallAabb::blocks_body` is
  stance-aware and uses `CONTACT_EPS` so the clamp plane is not re-read as a
  wall. `move_horizontal` refuses a step whose depenetration exceeds
  `PLAYER_RADIUS + CONTACT_EPS`, so a centre-inside case is a blocked step, not
  the historical forward teleport.

### Stance (`src/game.rs`, `src/collision.rs`, `src/input.rs`, `src/settings.rs`, `src/ui.rs`)

- `Stance::{Standing, Crouched}`; `CROUCH_HEIGHT = PLAYER_HEIGHT / 2 = 0.9`,
  `CROUCH_EYE_HEIGHT = EYE_HEIGHT / 2 = 0.8`. `Game::toggle_stance` anchors the
  feet by shifting the eye by the offset difference and refuses standing unless
  `head_clear_for(feet, PLAYER_HEIGHT)`.
- `Control::Crouch`, `KeyBindings.crouch` (`#[serde(default = "default_crouch_binding")]`,
  default `"C"`), `ACTIONS` 10 entries, `SettingsPage::Controls` item count 17,
  UI signature hash updated. The toggle is edge-latched in `Game` like Jump, so
  key repeat and held keys never re-toggle; pause/focus clears hold the latches
  until release.

### Water (`src/game.rs`, `src/level.rs`)

- Every land eye computation uses `Game::eye_offset()` (stance); the swim pose
  keeps its own buoyancy constants (`FLOAT_EYE_MARGIN = 0.12`,
  `SWIM_FLOOR_CLEARANCE = 0.55`, `SWIM_SINK_TERMINAL = 0.5`), so a stance change
  in water moves nothing (test).
- `Game::is_deep_water` requires feet depth > `WADE_DEPTH` **and** the eye at or
  below the surface swim band (`SURFACE_SWIM_EYE_MARGIN + FLOAT_EYE_MARGIN`), so
  entering water from above starts at the surface instead of in mid-air, and a
  climber releasing a ladder above the waterline jumps clear instead of being
  dragged to the float line.
- The stand-up depth is derived from the band (`EXIT_DEPTH = EYE_HEIGHT −
  band = 1.23 m` standing, `0.43 m` crouched), so a stand-up can never
  immediately re-enter swimming; a 0.6 m pool is waded standing, floats when
  crouched, and recovers to wading on uncrouch.
- `leave_water(sample)` re-derives the standing eye only when there is a
  coherent pose: the floor is within `EXIT_DEPTH` of the surface, the eye is
  near the surface and the stance fits. When the swimmer's virtual feet are
  inside the floor it stands if the clearance allows, otherwise it stays
  swimming rather than pushing the body through geometry; otherwise the player
  falls from the eye line with no upward snap.
- The swimmer's head clamp is referenced to the eye, not the virtual feet, so a
  floor-region rim beside the water is a wall, not an overhead (it previously
  dragged a surface swimmer to the pool floor).

### Ladders (`src/level.rs`, `src/loader.rs`, `src/game.rs`, `assets/levels/places_demo.json`)

- `LadderDef { x, z, width, depth, bottom_y, top_y, facing_degrees }` and the
  resolved `Ladders`/`Ladder` (footprint, facing vector, `approach_side`,
  `overlaps_disc`, `overlaps_body_y`, `climb_intent`); `MAX_LEVEL_LADDERS = 256`;
  `validate_ladders` rejects non-finite/zero/inverted/outside-room volumes.
- `Game::update_ladder` attaches on movement intent from the approach side and
  `climb_step` resolves `LADDER_CLIMB_SPEED = 2.2` m/s with collision-checked
  horizontal movement, `LADDER_INTENT_THRESHOLD = 0.3`, hold/back-away/jump/
  obstruction rules and a step-bounded top landing. Backing away detaches (not
  climbs down), and an obstruction clamps the rise without ever pushing the
  climber down. `bottom_y` is the attach reach; there is no climb-down pose.
- `assets/levels/places_demo.json` authors
  `{ x 19.35, z 11.7, width 0.6, depth 0.6, bottom_y -3.0, top_y -1.5,
  facing_degrees 90 }` at the ladder prop, and the `core:pool_ladder` placement
  is now `"solid": false` (the climb volume owns the interaction).

### Interfaces reusable by later runs

- `game::{CollisionWorld, Stance, LocomotionSnapshot, Game}`:
  `CollisionWorld::{from_level}`, `Game::{new, reset_level, collision_world,
  stance, is_crouched, is_climbing, body_height, eye_offset, feet_y,
  update_player_movement}`.
- `collision::{WallAabb, blocks_body, overlaps_disc, supports_center,
  highest_support_top, lowest_underside, resolve_player_collision_for_body}`.
- `level::{Ladders, Ladder, LadderDef, WaterVolumes, WaterSample, WalkableFloor,
  WalkableCeiling}`.
- `input::Control::Crouch`; `settings::KeyBindings::{crouch, ACTIONS}`.

### Verification performed

| Check | Result |
| --- | --- |
| Baseline `cargo test --workspace --no-fail-fast` (before edits) | 1138 passed, 0 failed, 9 ignored (425.40 s) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean (exit 0) |
| `cargo test --bin places -- game:: collision:: input:: settings:: ui:: loader::` | 248 passed, 0 failed, 1 ignored (snapshot; the parallel stream was still adding tests) |
| `cargo test --workspace --no-fail-fast` (final, after all edits) | 1196 passed, 0 failed, 9 ignored (436.09 s) |
| `PLACES_STAIRS_TRACE=… cargo test --bin places capture_stairs_walk_trace -- --ignored` | up: riser 0.2625, tread 0.3125, max abs eye step 0.30448 (one riser + one frame of pitch slope), 1 frame > 0.9 riser; down: max abs eye step 0.26254 (exactly one riser) |

The final workspace count includes the concurrent interaction stream's tests present
in the shared checkout (see the checkout note above); the baseline 1138 did not.

Focused regressions added in `src/game/tests.rs`: `standing_jump_lands_on_the_demo_desk_top`,
`a_prop_underside_blocks_the_head_and_a_crouch_fits_under`,
`test_controller_walks_off_a_deep_edge_and_falls`, `jumping_through_the_demo_doorway_never_teleports_or_clips`,
`repeated_jumps_under_the_demo_header_stay_bounded`, `landing_on_a_staircase_uses_the_rendered_tread`,
`the_demo_staircase_joins_the_hall_and_the_balcony`, `crouch_toggles_and_anchors_the_feet`,
`blocked_uncrouch_keeps_the_crouched_body`, `stance_changes_on_stairs_in_air_and_on_ladders_anchor_the_feet`,
`walking_off_the_demo_deck_falls_into_the_pool_and_swims` (jump-free drop),
`the_demo_pool_ladder_climbs_from_the_water_to_the_deck` (walks off the deck, falls,
swims to the ladder, attaches, climbs and lands on the deck),
`a_ladder_never_attaches_from_its_exit_side` (the minimal case that fails if the
approach-side rule is removed), `a_ladder_obstruction_holds_the_climber_in_place`,
`the_pool_ladder_releases_on_backing_away_and_jump`,
`a_floor_rim_never_drags_a_surface_swimmer_down`,
`stance_changes_in_water_do_not_move_the_eye`, `wading_uses_the_stance_eye_offset_on_the_real_step`,
`a_mid_depth_pool_wades_and_recovers_from_a_crouch`, `a_frame_hitch_does_not_tunnel_or_launch`,
`the_pit_carpet_hole_is_a_real_fall`; plus settings/input/loader/UI tests for the `C` binding
and ladder validation.

### Independent review

An independent regression review of the diff was run before finalization. It
confirmed the named tests, the focused results and the stairs trace, and found
issues that were then fixed and covered: the swimmer's head clamp read
floor-region rims as overheads and could drag a surface swimmer to the pool
floor; `support_at` could snap a swimmer whose virtual feet were inside a floor
up through it; `validate_ladders` could panic on a sub-2 mm footprint; the
ladder wrong-side and obstruction cases had only helper/vacuous coverage; and
the deck-fall test used Jump. The final verification below is from the fixed
tree.

### Checkout note

A concurrent interaction/trigger stream's uncommitted changes (`src/interact.rs`,
`Control::Interact`, `area_triggers`, UI row count) are present in this checkout
and were preserved; run 01 changed none of that work except one test-file clippy
allow (`src/input/tests.rs`). The verification numbers below are for the shared
tree; run 01's own changes are the controller, collision, level/ladder, crouch
binding, level data and docs described above.

### Unavailable runtime checks

- No interactive play-through or screenshot capture was performed: this
  environment runs headless (no GPU/window session), so `PLACES_CAPTURE` and a
  manual ladder/fall inspection were not possible. All run-01 evidence is
  deterministic controller/loader tests against the real `places_demo.json`,
  `home_showcase.json` and `level0_pit.json` data, plus the ignored stairs CSV
  diagnostic quoted above. A later validation run should capture the ladder
  climb, a pool-deck fall and a doorway jump on the GPU path.

## 4. Run 01 remaining issues / next-run dependencies

- **Animation run:** climbing reports `LocomotionState::Airborne` (there is no
  `Climbing` variant), and `LocomotionSnapshot` has no stance field, so a
  crouched player will use the standing idle/walk poses. Extend the snapshot
  (and the renderer's state map) when the animation run lands.
- **Stair soffits:** stairs/ramps contribute no collision boxes of their own;
  the walkable surface owns their footprint and the sides are bounded by the
  step rule, so the space under a flight is not reachable. A future geometry run
  adding open undercrofts should give them real underside colliders.
- **Ladder generality:** one ladder per attach (first footprint overlap), no
  diagonal/sloped ladders, no moving ladders, and no climb-down pose (backing
  away detaches); the prop must be `solid: false` for a clean exit.
  `assets/prop_proxies.json` and the catalog still describe the ladder as a
  solid prop for editor defaults; the demo placement overrides it.
- **Water:** the swim pose is a horizontal body with a fixed floor clearance;
  stance does not change it (deliberate). The swim entry band means a standing
  player wades to about 1.23 m of depth before swimming, and the stand-up
  threshold is derived from that band (see the guide). Underwater camera/fog
  remains absent.
- **Validation runs:** re-run the manual checklist in `docs/VERIFICATION.md`,
  including a GPU capture of the demo pool ladder and the Home stairs, and
  confirm `tests/test_package.py` (it still asserts the untouched
  `pool_showcase` fixture ladder is solid).
- Later runs that add level geometry should keep `ladders[]` volumes aligned
  with their props; `validate_level` rejects ladder volumes outside rooms but
  cannot know a prop moved.

## 5. Run 02 — map-authored interactions, labels and area triggers

### 5.1 Starting state and roles

Run 02 carried run 01's working-tree changes forward (same modified file set plus
this run's `src/interact.rs` and the two map files); nothing was committed,
branched, stashed or moved. Run 01's report gained an independent-review note and
a checkout note (which observed this run's interaction/trigger files already in
the tree); both are preserved above, and run 02 is the interaction/trigger stream
that note refers to. Three roles were used and are recorded here:

- **Interaction/input investigator** — mapped the input bits, binding
  persistence/sanitize, pause/menu/focus/mouse-capture gating, the UI controls
  page, and the text/UI pipeline the labels reuse. Its patch plan is what the
  run applied (with corrections from the lead: the Controls page retune is 11 px
  rows rather than 13 px, and label drawing lives in `main::submit_ui`).
- **Trigger/schema investigator** — proposed the id, action and trigger schema,
  located the 15 Pit holes and the demo's candidate props, and listed the struct
  literals and validation sites. The lead trimmed its catalog-name resolution
  (display names are map-authored with a model-id fallback) and made unsupported
  actions validation errors rather than accepted no-ops.
- **Independent instance-isolation reviewer** — adversarial review of duplicate
  models, id collisions, trigger-state desynchronisation and reset side effects.
  Findings and their fixes are recorded in §5.6.

### 5.2 Requirement checklist

Legend: [x] implemented and covered by a test, [~] implemented with a caveat,
[ ] not done.

#### Identity and actions

- [x] Stable per-instance ids for placed props/entities, light fixtures and area
      triggers, distinct from asset/catalog ids; deterministic documented
      defaults; duplicates and malformed values are named errors.
- [x] References (label targets) resolve against the authored namespace and an
      unknown target is refused with the id in the message.
- [x] Old maps and old settings load unchanged; no mutable object state is keyed
      by model filename or catalog id.
- [x] One typed action dispatcher for object interactions and triggers, with
      per-map/per-instance actions and targets; bounded composition (≤ 8),
      deferred batches and at-most-one trigger batch per frame.
- [x] Label toggling and reset-to-start implemented.
- [~] Animation actions are a documented, validation-rejected integration point;
      no existing per-instance animation action was available to connect. Audio
      has no subsystem at all and is rejected by name (no silent success).

#### E interaction

- [x] E aims at the nearest eligible object in a documented 2.5 m reach (cap
      4.0 m), from the actual stance-aware eye, respecting occluding geometry.
- [x] One press per key edge; held keys, OS repeat, menus, pause and lost focus
      never re-fire.
- [x] `Interact` binding beside the movement keys, default `E`, rebinding and
      persistence via the existing settings mechanism; old files migrate to E
      without overwriting a customized binding.
- [x] Demo props and the Spooner-Man entity author label toggles; only the
      pressed instance changes; duplicate pool chairs are independent.
- [x] Labels follow the instance transform, use the existing UI text pipeline,
      and hide behind occluders.

#### Area triggers and reset

- [x] Authored volumes with enter semantics, leaving re-arm, cooldowns, `once`,
      target validation and safe action ordering.
- [x] Swept crossings catch a fast fall through a thin band, not just endpoint
      overlap.
- [x] Reset returns to the authored spawn and facing, clears velocity and
      reconciles grounded/water/ladder/stance state, suppresses held keys until
      release and re-seeds triggers so a teleport sweeps nothing in between.
- [x] All 15 intended Pit carpet holes have a reset trigger; walking the carpet
      between them stays safe. Repeated resets stay bounded.

#### Acceptance

- [x] Focused tests cover reach, occlusion, crouched targeting, rebinding,
      repeat suppression, independent duplicate labels, entity labels, thin
      volume crossings, cooldown/re-arm, invalid references, bounded dispatch
      and repeated Pit resets.
- [x] Map-authored examples documented in the authoring guide and shipped in the
      demo/Pit.
- [x] Dispatcher/identity contracts recorded for animation and switch assets
      (`docs/ARCHITECTURE.md`, "Interaction identity and action contracts").
- [~] No interactive play-through/GPU capture in this headless environment;
      evidence is deterministic controller/loader/UI tests against the real
      maps plus the Python validator.

### 5.3 Implementation

- **Schema (`src/level.rs`).** `PropDef` gained `id`, `display_name` and
  `interaction`; `LightFixtureDef` gained `id`; `LevelDef` gained
  `area_triggers`. `ActionDef` is an internally tagged, closed enum
  (`toggle_label`, `reset_to_start`, `play_animation`, `play_audio`);
  `PropInteractionDef` carries `prompt`/`reach`/`actions`; `AreaTriggerDef`
  carries the footprint, optional vertical bounds, actions, `cooldown_seconds`
  and `once`. `AreaTriggers::from_level` resolves the runtime form (ids,
  normalised bounds, floor-derived verticals) and skips malformed entries, like
  water volumes and ladders. `LevelDef::{prop,light,area_trigger}_instance_ids`
  are the single deterministic id source.
- **Validation (`src/loader.rs`).** `validate_instance_ids` enforces well-formed,
  unique ids across props/fixtures/triggers and validates prop interactions;
  `validate_area_triggers` enforces count, finite/positive geometry, non-negative
  cooldown, room overlap, resolved top above bottom, and 1..8 actions.
  `validate_action_list`/`validate_action` bound composition, resolve
  `toggle_label` targets against the prop id list, and reject
  `play_animation`/`play_audio` by name as not implemented. Old maps have none of
  the new keys, so every validator is a no-op for them.
- **Interaction module (`src/interact.rs`, new).** `Interactable` (id, display
  name, prompt, reach, anchor, bounds, own collision box, actions) and
  `Interactables::from_level` resolve aimable instances with the same size
  contract as collision, plus every prop named as an explicit `toggle_label`
  target as a label-only instance (empty actions, never aimable).
  `nearest_target` implements reach + occlusion; `ray_aabb_entry` /
  `segment_overlaps_aabb` (`src/collision.rs`) are the shared ray/swept maths;
  `append_world_labels` projects visible labels and the aimed-at prompt through
  the existing `ui::draw_text`/`render_ui` path; `view_direction` matches the
  render camera.
- **Controller (`src/game.rs`).** `CollisionWorld` and `Game` carry
  `interactables` and `triggers` (private, behind accessors, with parallel
  private state); `Game` owns `interact_latched`/`interact_pressed`, per-trigger
  state (including a `pending` deferral flag), per-instance `label_visible`, the
  stored spawn and the reset counter. `update_player_movement` runs the Playing
  frame (which returns the post-stance swept origin), then `update_triggers`.
  `dispatch_actions` is the single dispatcher (`DispatchReport`);
  `dispatch_interaction` resolves the aimed instance and passes it as the
  implicit target; an explicit target must resolve on its own. `reset_to_spawn`
  uses `clear_run_state` with held-key suppression and re-seeds triggers.
- **Input and settings (`src/input.rs`, `src/settings.rs`, `src/ui.rs`).**
  `Control::Interact` (bit 10), `KeyBindings.interact` default `E`, a Controls
  row (18 rows, 11 px spacing) and the UI signature hash. `sanitize` now leaves
  an action unbound when its default key is already taken instead of creating a
  duplicate.
- **Frame loop (`src/main.rs`).** A focus loss releases gameplay inputs; the
  press is consumed after movement and dispatched only while focused;
  `submit_ui` appends world labels/prompt to the cached menu vertices.
- **Maps.** `assets/levels/places_demo.json` gives the front desk, water cooler,
  two pool chairs (duplicate model), Home plant and Spooner-Man ids, display
  names and label interactions; Spooner-Man authors `size [0.7, 1.8, 0.7]` so the
  aim bound covers the entity. `levels/level0_pit.json` gains 15
  `pit_hole_<n>` triggers (1.6x1.6 footprints at the recessed regions' own
  coordinates, `bottom_y -3.2`, `top_y -0.05`, `reset_to_start`,
  `cooldown_seconds 0.5`).

### 5.4 Dispatcher/identity contracts for later runs

Recorded in `docs/ARCHITECTURE.md` ("Interaction identity and action
contracts"). In short: identity is per placed instance (`prop_instance_ids`,
authored or `<model-short>_<n>`), never model/catalog keyed; `Interactable` is
the aimable view resolved at load plus label-only targets named by an explicit
action; `Game::dispatch_actions(&[ActionDef], actor)` is the single dispatch
entry point; label state is a per-interactable `Vec<bool>`; `AreaTriggers` +
`Game::update_triggers` own enter semantics with one batch per frame and pending
deferral; run 3 implements `play_animation` by extending `validate_action` and
one `dispatch_actions` arm, and must feed live entity transforms into the same
anchor.

### 5.5 Verification performed

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean (exit 0) |
| Focused `cargo test --bin places -- game::tests::` | 70 passed, 0 failed, 1 ignored |
| Focused `cargo test --bin places -- loader::tests::` | 78 passed, 0 failed |
| Focused `cargo test --bin places -- settings:: input:: level::tests:: interact:: collision:: ui::` | 151 passed, 0 failed |
| `python3 tools/assets/validate.py` over all levels (demo + Pit + fixtures) | 0 errors, 0 warnings |
| `cargo test --workspace` (final) | 1195 passed, 0 failed, 9 ignored |
| `cargo test --workspace` baseline before run 02 edits | 1184 passed, 0 failed, 9 ignored (shared tree carrying run 01's changes plus early run-02 work) |
| `cargo test --workspace` after the main run-02 implementation (before review fixes) | 1188 passed, 0 failed, 9 ignored |

New deterministic coverage includes:
`interaction_press_latches_once_and_never_fires_while_paused`,
`interaction_targeting_is_nearest_in_reach_and_duplicates_are_independent`,
`crouched_eye_height_and_occlusion_respect_geometry`,
`labels_reset_on_level_load_and_survive_reset_to_start`,
`the_demo_authors_entity_and_duplicate_prop_labels`,
`area_triggers_enter_once_rearm_and_honour_cooldown_and_once`,
`a_fast_fall_through_a_thin_trigger_band_is_caught`,
`reset_to_start_clears_state_and_never_sweeps_the_teleport`,
`holding_jump_across_a_reset_does_not_launch_or_stick`,
`dispatch_reports_missing_targets_unsupported_actions_and_stops_after_reset`,
`the_pit_carpet_holes_reset_and_the_carpet_is_safe`,
`the_pit_carpet_hole_is_a_real_fall` (updated for the reset trigger),
`interact_rebinding_repeat_suppression_and_release_all`,
`interact_migration_never_overwrites_a_customized_e_binding`,
`explicit_targets_can_be_label_only_props`,
`a_crouch_toggle_in_water_does_not_fabricate_a_trigger_crossing`,
`a_swept_crossing_of_a_later_trigger_is_deferred_not_dropped`, plus
loader/level schema and validation tests and the `interact`
projection/label/occlusion tests.

### 5.6 Independent review findings and resolutions

The instance-isolation reviewer confirmed no run state is keyed by model,
fixture, catalog id or file name; duplicate-model instances get distinct
indices; trigger state is re-seeded on every trigger-vector installation; and
the demo/Pit data match their intended geometry. It found one contract breach
reachable from a validated map and several edge cases, all fixed and covered:

1. **Unresolvable explicit target silently retargeted the actor** (medium).
   Validation resolved `toggle_label` targets against all props, but the runtime
   only knew props with interactions, and `.or(actor)` meant a target naming a
   non-interactable prop toggled the *actor's* label (or no-opped from a
   trigger). Fixed: the runtime now keeps an explicit target and an omitted
   target distinct (an unresolved explicit target counts `missing_targets` and
   never falls back), and `Interactables` includes every prop named as a target
   as a label-only instance, so a validated target always resolves. Covered by
   `explicit_targets_can_be_label_only_props`,
   `test_validate_accepts_a_label_only_target` and the no-fallback assertions in
   `dispatch_reports_missing_targets_unsupported_actions_and_stops_after_reset`.
2. **Crouch/stand while swimming fabricated a vertical trigger sweep** (low).
   The swept origin was captured before the stance edge; on land the feet anchor
   makes that identical, but in water the eye stays and the feet move 0.8 m.
   Fixed: the Playing frame returns the origin captured *after* the stance edge.
   Covered by `a_crouch_toggle_in_water_does_not_fabricate_a_trigger_crossing`.
3. **Occlusion self-exclusion could skip a genuine barrier** (low). Any wall
   overlapping the target's bounds was skipped as "self". Fixed: `Interactable`
   records its own collision box when solid, and only an exact box match is
   skipped; a barrier that merely intersects the bounds occludes. Covered by the
   barrier case in `nearest_target_respects_reach_and_occlusion`.
4. **A one-frame crossing of a later trigger could be dropped** (low). At most
   one batch runs per frame, and later triggers were returned past without
   refreshing their state. Fixed: an entry (or swept crossing) sets a per-trigger
   `pending` flag that dispatches on a later frame. Covered by
   `a_swept_crossing_of_a_later_trigger_is_deferred_not_dropped`.
5. **Misleading "deferred" wording for post-reset batch actions** (low). The
   remainder of a batch after `reset_to_start` is discarded, not queued; the
   code comments now say so (the trigger-level pending flag is the only deferred
   path).
6. **Public `triggers`/`interactables` fields could desynchronise their private
   parallel state** (latent). Both are now private behind `triggers()` /
   `interactables()`; only `Game::new`/`reset_level` install them, each followed
   by re-seeding.

The reviewer's remaining notes (duplicate ids and blank targets in hand-built
`LevelDef`s, and the defensive `prop_{index}` id fallback) are unreachable
through the loader and fail safe; they are documented rather than changed.

### 5.7 Unavailable runtime checks

- Still headless: no GPU/window session, so `PLACES_CAPTURE` of a label/prompt
  and an interactive E press were not possible. A later validation run should
  capture a toggled label, a crouched aim under a low beam and a Pit fall with
  the hardware path. Labels are part of the submitted UI vertices, so
  `PLACES_CAPTURE` will include them when run.

## 6. Run 02 remaining issues / next-run dependencies

- **Animation/audio actions.** `play_animation` and `play_audio` parse but are
  rejected by validation. Run 3 implements the former; the audio route needs a
  subsystem that does not exist. The `DispatchReport.unsupported` path is the
  runtime safety net for programmatically constructed actions.
- **Entity anchors are level-derived.** `Interactable.anchor`/`bounds` come from
  the placed transform at load. The demo's entities do not move today; when run 3
  animates or moves one, the label anchor must come from the renderer's live
  transform, keyed by the same instance id.
- **Aim bounds use the collision size contract, not the catalog size.** A prop
  with no authored `size` is aimable as `[0.6, 0.9, 0.6]`; author `size` on tall
  or wide interactables (the demo does for Spooner-Man).
- **`once` triggers re-arm on any `reset_to_start`.** Documented and tested;
  a map that wants a run-lifetime latch should not combine it with resets.
- **Keyboard is gated by app state and focus, not by mouse capture.** Relative
  mouse mode follows focus/state, but a deliberately uncaptured window can still
  dispatch E; this matches how Jump/Crouch already behave.
- **Python validator.** `tools/assets/validate.py` now mirrors the id, action and
  trigger rules (unimplemented actions rejected, unknown targets rejected, 0
  errors over the shipped + drop-in levels), but the Rust loader remains the
  authority.
- **Level editor.** `level-editor/js/` still does not model ids, interactions or
  area triggers and drops unknown keys on save; edit those as JSON.
- **`levels/level0_pit.json` is gitignored** (a per-user drop-in). The Pit tests
  read it from disk; on a checkout without it those tests fail, as the run-1
  Pit test already did.

## 7. Run history

- Run 01: gravity 9.8 + desk-sized jump; real falls and prop-top landings;
  doorway/head collision; stance and `C` crouch; water transition fixes;
  `ladders[]` primitive and the demo pool ladder repair.
- Run 02: per-instance ids; `E` interaction with reach, occlusion
  and crouch-aware aim; floating label toggles with duplicate-model isolation;
  the typed action dispatcher with bounded/deferred batches; area triggers with
  swept crossings, cooldowns and `once`; reset-to-start across ground/water/
  ladder/stance; the 15-hole Pit reset integration; animation/audio integration
  points.
- Run 03 (partial, this report): real animation clips authored into the
  Spoonerman GLB, a Blender-confirmed skin-weight repair, CUBICSPLINE and morph
  support in the importer, and an explicit per-instance cue player with
  loop/once/pause/hold/crossfade. Entity routes and the demo walk/sit were
  investigated and designed but **not implemented**; see §8.8 for the exact
  next-run starting point (completed by run 04, §9).
- Run 04: completed the run-03 route/pose/cue integration (map-authored
  `routes[]`, stride-aware walk/run clip selection, per-instance
  `play_animation` with live entity transforms and anchors); created and
  registered the animated rat, the grey concrete three-pose mannequin and the
  articulated human skeleton; added the pure-Python entity toolkit, the
  `entity_showcase` fixture, editor proxies/thumbnails and the run-04
  regressions.
- Run 05: created and registered the remaining props (stop sign, luminous exit
  sign, hanging ball light, wall switch, CRT TV, the home table setting and the
  potted table plant, the pool's rubber duck); added the `toggle_animation`
  action with a reversible scrub cue and rigid (skinless) prop animation; added
  the floating-prop component and its validation; integrated the demo switch,
  lights, place settings and floating duck; and removed the retired level
  editor and all of its machinery (see §10).
- Run 06: added data-authored arc walls and circular pillars with
  segment-derived collision and world-scale tiling; repaired the demo stair
  handrails, the doubled guardrail end post, sloped-rail UVs and
  automatic-baseboard corner ends; raised the pool tile sheen; added the
  ceiling-vent decal with per-room ceiling tile frames and grid snapping; and
  delivered the read-only `--check-geometry` map checker with fixtures, demo
  and Pit reports (see §11).

## 8. Run 03 — animated GLB playback and Spoonerman asset (partial)

### 8.1 Starting state and roles

Run 03 carried forward runs 01–02's working tree unchanged (still no commits,
branches, stashes or worktrees). Two read-only investigators were used, and
their reports are the basis for the work that follows:

- **GLB/animation investigator** — audited `src/gltf.rs` against the glTF 2.0
  spec and the pinned Blender-exported asset, and located the renderer's
  character path, clip selection, GPU upload and culling/shadow/reflection
  involvement.
- **Entity-navigation investigator** — mapped the collision/query APIs, the
  interaction/trigger dispatcher, the demo geometry, six verified sitting
  destinations, and a proposed route schema/runtime.

An **independent animation/rendering reviewer** was run on the diff after
integration (session `ses_f2344b4e3ffefSGLXPHskZBngR`); its findings are folded
into §8.6. The lead applied every repository edit.

### 8.2 What the audit established

- The shipped `spooner-man.glb` is a glTF 2.0 GLB (Blender I/O 5.2.40), one
  scene, 28 nodes, one 26-joint skin, one mesh with 3 skinned primitives,
  3 embedded 256×256 PNGs, no extensions, no morph targets and (before this
  run) no clips. The BIN chunk was 450,412 bytes.
- **The shipped skin weights were degenerate.** Under the spec mapping
  (`JOINTS_0` indexes `skin.joints`), under 2% of the total vertex weight
  reached any leg bone; the lowest front-paw vertices were weighted
  `chest 0.58 / neck 0.42` and the rear paws to `neck/head`. Blender 5.2.2's
  own importer (`bpy.ops.import_scene.gltf`) reported the same vertex groups,
  so this is an asset defect, not a parser defect. The rest pose was unaffected
  (any joint/IBM pairing is identity at rest), which is why the model looked
  correct while no leg animation could move the feet.
- Importer gaps confirmed against the spec: CUBICSPLINE rejected; morph
  targets rejected; `inverseBindMatrices` required although the spec makes it
  optional; `asset.version` unchecked; node quaternions not normalized;
  a second used skin, sparse accessors, external/data-URI images, per-node
  morph weight overrides and every extension except
  `KHR_materials_emissive_strength` rejected (some with clear messages, some
  not); accessor `byteOffset`/`byteStride`/normalized integer decoding already
  correct.

### 8.3 Spoonerman asset (delivered)

`tools/props/animate_spooner_man.py` is the reproducible source path. It
preserves every existing buffer view offset and length (mesh, textures,
hierarchy, skin, inverse binds, rest pose) and appends only animation data. It
also detects and repairs the degenerate skin weights with a deterministic
nearest-segment, two-bone blend (`sigma 0.02 m`, the central `root` bone
excluded, second influence dropped below a 0.12 share), rewriting only the
`JOINTS_0`/`WEIGHTS_0` bytes in place.

- Repair result: leg weight share **1.5% → 42.3%**; bind pose unchanged
  (positions and rest render identical).
- Authored clips (LINEAR, sampled per key, merged into `asset.extras`
  `places_entity_clips` for idempotent regeneration):

| clip | duration | keys | pose |
| --- | --- | --- | --- |
| `idle` | 4.0 s loop | 17 | breathing spine/chest/neck/head, tail sway |
| `walk` | 0.6 s loop | 25 | diagonal-pair slow gait, head/body bob, tail sway |
| `sit_down` | 1.6 s once | 17 | stand→sit with a mid-transition pelvis lift |
| `sit_idle` | 5.0 s loop | 21 | solved seated pose + restrained breathing |
| `stand_up` | 1.4 s once | 15 | sit→stand, same lift |

- Seat solve: the seated pose was solved against the skinned mesh with a
  coarse-to-fine search (`sitsearch.py`, see §8.5): pelvis pitch −20°, rear
  leg (−45°, −27°, −12°), front leg (−7°, −5°), contact spread 1.5 cm, pelvis
  drop −0.1323 m. `sit_idle` lowest vertex is exactly on the floor plane
  (y min 0.000, head top 0.347 m vs 0.389 m standing).
- Walk stride measured from the paw joint: 0.155 m per cycle → reference
  speed 0.259 m/s; the engine constant `WALK_REFERENCE_SPEED_MPS = 0.26` and
  the tool's marker agree.
- GLB grew 461,332 → **532,896 bytes** (134 channels, 5 clips; sampler input accessors declare spec-required min/max).
- Commands and checks:
  - `python3 tools/props/animate_spooner_man.py --report` (write + bounds/stride report)
  - `python3 tools/props/animate_spooner_man.py --check` (clip table + BIN growth)
  - Blender 5.2.2 headless reimport rendered start/mid/end frames for all five
    clips (15 PNGs under the run's scratch directory): the sit renders show the
    front legs planted, the haunches folded and the body lowered; the walk
    renders show diagonal leg offsets. This is asset-level evidence, not engine
    playback.
- Docs updated: `assets/entities/spooner-man/README.md`,
  `assets/entities/README.md`, the two tool docstrings, and the importer module
  header.

### 8.4 Importer/runtime (delivered)

`src/gltf.rs`:

- `AnimationInterpolation::CubicSpline` parses the spec's 3-tuple layout and
  the runtime evaluates the Hermite form with `delta * tangent` terms.
- Morph targets: `POSITION` required, `NORMAL`/`TANGENT` optional; defaults
  come from `primitive.weights`, else `mesh.weights`, else zero; defaults bake
  into the bind-pose vertices and the transformed deltas are stored parallel
  to the model's vertices (`PropModel::morph_targets`/`morph_weights`); the
  `weights` animation path drives them, verified by
  `morph_weight_channels_animate_the_target_weights`.
- `inverseBindMatrices` is optional (identity default); `asset.version` must
  be `2.0`; node quaternions are normalized (zero-length ones are rejected);
  per-node morph weight overrides are rejected by name.
- New ceiling `MAX_PROP_MORPH_TARGETS = 32`.

`src/render/common/character.rs`:

- `PoseCue::{Idle, Walk{speed_mps}, Clip{name, once, paused}}` with
  `update_cued`/`resolve_cue`/`sample_active_cue`; cues crossfade from the
  current pose over the existing 0.18 s constant, keep per-instance time,
  pause, hold a one-shot's last key and report completion once
  (`take_cue_finished`).
- Clip lookup is case-insensitive by name; `WALK_REFERENCE_SPEED_MPS` scales
  walk playback so the authored stride matches the route speed.
- Cubic/weights sampling lives in `Channel::sample_component`/`Clip::sample`;
  morph buffers are blended alongside the pose.

### 8.5 Offline-compute performance (requested)

The heavy scratch solver is `sitsearch.py` (macOS `spawn` pool, importable
worker functions, guarded entry point, `--workers N` with `--workers 1` as the
serial reference, progress lines, a JSON checkpoint per stage, coarse-to-fine
rather than one brute-force grid, and a per-joint precomputed skinning matrix
so each candidate evaluates only the Y row of ~3,000 vertices):

| measurement | result |
| --- | --- |
| 1,056-candidate sample, serial | 1.75 s |
| 1,056-candidate sample, `--workers 12` | 0.32 s (**5.49×**) |
| Same sample, `--workers 8` | 0.35 s (5.08×) |
| Full two-stage solve, 12 workers | 2.6 s wall, 25.8 s user CPU |
| Full solve, 1 worker (reference) | same result, serial semantics preserved |

Peak pool size was 12 (the machine's core count), workers only read the GLB and
return small tuples, the parent alone writes the checkpoint, and no worker
touches Blender or repository outputs. Earlier brute-force probes were replaced
by the coarse-to-fine search; total user CPU for the whole run fell from
~800 s (a mis-grouped first attempt) to ~26 s.

### 8.6 Verification performed

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean (exit 0) after the new lints were fixed by splitting/bundling, not allows |
| `cargo test --bin places -- gltf:: render::common::character::` | 38 passed, 0 failed |
| `cargo test --bin places -- render::common::character::tests::` | 13 passed, 0 failed |
| Blender 5.2.2 headless reimport + renders of all five clips | 15 frames rendered; sit/walk poses observed |
| GLB structure via `--check` | 5 clips present; base BIN 450,412 B, total 491,248 B |
| Repo toolkit reader (`tools/props/glb.py`) | 28 nodes, 1 skin, 5 animations (`idle` 26 ch, others 27 ch) |
| `cargo test --workspace --no-fail-fast` (after the main run-03 implementation, before review fixes) | 1199 passed, 0 failed, 9 ignored (425.93 s) |
| `cargo test --workspace --no-fail-fast` (final, after the review fixes) | **1202 passed, 0 failed, 9 ignored** (428.48 s); run-02 final was 1195, so run 03 added seven net tests and regressed none |

New focused coverage: `cubic_spline_sampling_hits_endpoints_tangents_and_midpoints`,
`explicit_clips_hold_once_pause_and_resume`,
`morph_weight_channels_animate_the_target_weights`,
`a_one_shot_holds_its_last_pose_while_the_next_cue_fades_in`,
`cue_completion_does_not_leak_and_can_restart`,
`linear_rotation_keys_take_the_shortest_arc`, the shipped-asset test now
asserts the five clips and the repaired leg-weight share, plus the asset
version and unknown-interpolation rejection updates.

### 8.7 Independent review findings and resolutions

The independent animation/rendering reviewer read the diff, ran clippy and the
focused tests, and built throwaway probes against a temp copy of the crate.
Confirmed bugs and their fixes:

1. **One-shot to loop crossfade started at frame 0 (high).** A held one-shot
   sampled as a looping source wrapped `time == duration` back to 0. Fixed:
   `cue_previous` records the one-shot flag and the previous clip is sampled
   clamped; the first cue also crossfades from the pre-cue pose buffers.
   Covered by `a_one_shot_holds_its_last_pose_while_the_next_cue_fades_in`.
2. **Multi-target `weights` channels were rejected (high).** The row-count
   check omitted the per-target factor. Fixed; the model total is now bounded
   by `MAX_PROP_MORPH_TARGETS_PER_MODEL = 128`. A parser fixture with two
   morph targets is still missing (see §8.10).
3. **LINEAR rotations were component-wise lerped (medium).** Antipodal or
   wide-spaced keys could take the long arc. Fixed with shortest-arc
   `Quat::slerp`; covered by `linear_rotation_keys_take_the_shortest_arc`.
4. **A finished cue's completion leaked across cue changes (low/medium), and
   an identical one-shot could not replay.** Fixed: the change branch clears
   the flag, and `restart_cue` is the explicit replay hook. Covered by
   `cue_completion_does_not_leak_and_can_restart`.
5. **Looping cue time was unbounded** (precision drift over long sessions) and
   a non-finite sample time could write a NaN pose. Both fixed in
   `update_cued`/`Clip::sample`.
6. **The committed asset marker denied its own repair**, and sampler input
   accessors lacked spec-required `min`/`max`. Both fixed in
   `animate_spooner_man.py`; the regenerated GLB records
   `skin_repair.repaired: true` and time bounds `[0, 4]`.

The reviewer also confirmed as correct: the CUBICSPLINE Hermite form, tuple
indices and endpoint clamping; the rest-pose/morph-default reset and
loop-vs-clamp split; `finish_morphs` tuple order and offsets; the per-vertex
default bake in both append paths; the Python tool's byte/offset preservation,
monotonic BIN and deterministic idempotent reruns; the unchanged bind pose;
and the absence of new panic paths.

Remaining reviewer notes accepted for a later run: empty-index morph
primitives drop mesh defaults; a mesh visited by two nodes keeps independent
ranges but a weights channel addresses the first visit only; morph
normals/tangents are parsed but the vertex-lit character path does not consume
normals; there is no parser-level multi-target morph fixture.

### 8.8 NOT delivered in run 03 (honest scope statement)

The run's headline acceptance — Spoonerman walking a map-authored route with
six sitting stops through the actual collision world and the demo map — is
**not implemented**. The importer, the per-instance clip player and the asset
are in place; the missing integration is:

1. **Route schema** (`src/level.rs`): `RouteStepDef` (`move_to`, `face`,
   `wait`, `play_once`, `hold_pose`, `loop`), `EntityRouteDef`,
   `LevelDef.routes`, `MAX_LEVEL_ROUTES`/`MAX_ROUTE_STEPS`, and
   `loader::validate_routes` (finite waypoints on real floors, no solid/wall
   overlap, non-empty clips, bounded speeds/waits).
2. **Route runtime** (`src/entity.rs` or in `Game`): fixed-substep movement
   reusing `resolve_player_collision_for_body`, `WalkableFloor::walk_height_at`
   and `PLAYER_STEP_HEIGHT`; blocked segments stop and report instead of
   teleporting; per-entity deterministic state; reset-to-start handling;
   `PoseCue` output per frame.
3. **Renderer plumbing**: `Character` instance ids + `set_transform`,
   `CharacterScene::update` taking per-entity cues and applying live
   transforms/bounds, `WgpuCharacters::sync` writing the group-3 environment
   matrix on transform change, and morph deltas applied in
   `WgpuCharacters::fill_vertices` (the animator exposes
   `morph_weight_delta`, but the fill does not consume it yet).
4. **Gameplay glue**: `ActionDef::PlayAnimation` validation and dispatch to a
   route/entity cue, live interactable anchors for moving entities (the run-02
   contract), `main.rs` passing cues to `update_characters` and feeding
   one-shot completion back.
5. **Demo content**: the six-stop route in `assets/levels/places_demo.json`
   and a trigger-started `play_animation` example.
6. **Docs**: `docs/MAP_AUTHORING_GUIDE.md` route section,
   `docs/ARCHITECTURE.md` animation contract, `docs/RENDERER.md` cue playback,
   `tools/assets/validate.py` route validation, and the tests that still
   assume `play_animation` is rejected
   (`src/loader/tests.rs:2754`, `tools/assets/validate.py:76`).

The entity-navigation investigator's report (session
`ses_f237e0083ffdLO2oWpaquqe2yp`) contains the verified six-stop waypoint
polyline, the per-segment floor heights (the only vertical transitions are the
five 0.30 m floor-region risers in the x19..24 hall and the Home staircase),
and the collision-box checks; it is the recommended starting point for the
next run.

### 8.9 Unavailable runtime checks

Still headless: no GPU/window session, so `PLACES_CAPTURE`, an interactive
E press, and actual engine playback of the sit clips were not possible. The
animation evidence is the Blender reimport/render set plus deterministic
loader/animator tests; engine-side route playback remains unverified until the
integration in §8.7 lands and a GPU run captures timed frames.

### 8.10 Run 03 remaining issues

- Multi-skin documents remain unsupported (one used skin per model); sparse
  accessors, external/data-URI images, non-PNG images, and extensions other
  than `KHR_materials_emissive_strength` remain rejected with named errors.
- Animated morph weights are sampled and stored, but the renderer's vertex
  fill does not apply them yet (§8.8.3); morph normals/tangents are parsed but
  the vertex-lit character path does not consume normals at all.
- The clip tool's seat solve and skin repair are scratch-tool driven; the
  chosen constants are committed in `animate_spooner_man.py` and covered by
  `--check` + the shipped-asset test, but re-solving requires the scratch
  `sitsearch.py` (not committed).
- `tools/props/build.py`'s `--force` guard still protects the now-animated
  asset; the entity READMEs no longer claim zero clips.
- The walk clip's 0.26 m/s reference speed is tuned to the measured stride;
  a future gait change must update both the tool constant and
  `WALK_REFERENCE_SPEED_MPS` together.



## 9. Run 04 — entity routes, pose selection and the rat / mannequin / skeleton

### 9.1 Starting state and roles

Run 04 carried forward runs 01–03's working tree unchanged (still no commits,
branches, stashes or worktrees). The audit before any edit found run 03's
headline integration genuinely missing — no `LevelDef.routes`, no
`Game`-side route runtime, `CharacterScene::update` took only the player's
`LocomotionSnapshot`, `play_animation` was still rejected by `validate_action`
and counted as `unsupported` by the dispatcher — so **run 04 implemented the
run-03 route/pose/cue integration as its necessary prerequisite** before
creating the three new assets. Three specialists and one reviewer were used:

- **Creature/modeling specialist** — the animated rat mesh, rig, weights and
  three clips (`build_rat.py` proposal under `target/entity-specialists/rat/`).
- **Modeling specialist** — the grey concrete mannequin with its three poses
  (`build_mannequin.py`).
- **Humanoid-rig specialist** — the articulated skeleton with standing, floor
  and chair poses, including the analytic chair-seat fit
  (`build_skeleton.py`).
- **Independent asset/playback reviewer** — adversarial review of the diff,
  the assets and the parallel tooling; findings in §9.9.

All repository edits were applied by the lead; the specialists wrote only under
`target/entity-specialists/` (gitignored). The lead created the shared tooling
(`tools/entities/rig.py`), imported/adapted the three build scripts, and owns
the fixture, catalog, docs and tests.

### 9.2 Requirement checklist

Legend: [x] implemented and covered by a test, [~] implemented with a caveat,
[ ] not done.

#### Route/pose prerequisite (run 03's missing integration)

- [x] `routes[]` schema (`level::EntityRouteDef`, `RouteStepDef`
      `move_to`/`face`/`wait`/`play`, route `loop`) with `MAX_LEVEL_ROUTES`
      (256), `MAX_ROUTE_STEPS` (64), speed/wait/play bounds.
- [x] `entity::EntityRoutes` resolved from the level by placed-instance id;
      `Game` owns the parallel deterministic `RouteState`s and advances them
      in fixed 1/60 s substeps against the same walls/floor the player uses
      (`ENTITY_STEP_HEIGHT_M = 0.3`, 0.1 s clamp per frame).
- [x] A blocked step stalls in place and reports once; it never tunnels,
      teleports or leaves the walkable floor.
- [x] loader validation samples every straight segment at the entity's own
      (wider-axis) footprint and refuses off-floor waypoints, geometry
      crossings, `solid: true` props, duplicates, unknown ids and malformed
      steps. Python `tools/assets/validate.py` mirrors the structural rules.
- [x] `play_animation` implemented (target + `clip` + `loop`), per-instance
      override beats the route cue, unknown targets never fall back to the
      actor, `reset_to_start` clears overrides and re-seeds every route.
- [x] Per-clip metadata parsed from `asset.extras.places_entity_clips`
      (`loop`, `reference_speed_mps`, `kind`); `PoseCue::Walk` picks `run` at
      1.5× the walk reference and plays `speed / reference`; the run-03
      Spoonerman marker still resolves via its top-level
      `walk_reference_speed`.
- [x] `Game::entity_frames()` carries live `(position, yaw)` and cue per
      instance id; `CharacterScene::update` matches by id, `Character::set_pose`
      rebuilds the placement matrix/bounds, and `WgpuCharacters::sync` rewrites
      only the moved character's group-3 environment and bounds.
- [x] Routed entities' interactable anchors/bounds are recomputed from the
      live route state and the instance's own size every frame (labels and `E`
      follow the character, and a route turn re-orients the aim box).
- [x] `main.rs` passes `game.entity_frames()` to `update_characters`; a
      character with no frame still follows the locomotion snapshot.

#### Grey concrete mannequin

- [x] Recognizable grey concrete human lay figure (1130 tris, 23 joints, one
      embedded concrete sheet), real material painted with `tools/props/tex.py`.
- [x] Three selectable poses on **one** rig: `pose_stand`,
      `pose_arms_up` (hands to 2.105 m), `pose_arms_forward` (hands to
      `z = +0.675 m`), all single-key holds with identical proportions/joints;
      selected through `play_animation` or route `play` steps.
- [x] Engine-path proof: `the_shipped_mannequin_selects_its_poses_through_cues`
      drives the real `CharacterAnimator` and asserts hand reach, grounded
      feet and bind-equivalent stand.

#### Animated rat

- [x] Recognizable rat with articulated legs/paws, head, ears and a
      five-segment tail (856 tris, 25 joints).
- [x] Genuinely looping `idle` / `walk` / `run`; walk/run key the legs, body
      and tail with paw contact windows (offline contact check, feet within
      2 mm of the floor) and loop-closed channels.
- [x] Documented speed/loop metadata: measured `walk` 0.1985 m/s, `run`
      0.5731 m/s declared in `asset.extras.places_entity_clips` and
      re-measured by both the build tool and the shared validator; stance
      slide ≤ 0.2 mm (walk) / 1.7 mm (run) at the declared speeds.
- [x] Route demo at matching speeds in `entity_showcase.json`; engine-path
      `the_shipped_rat_walks_and_runs_with_declared_reference_speeds` verifies
      the paw lift and floor contact through the cue path. No player sprinting.

#### Articulated skeleton prop

- [x] Recognizable stylized skeleton: skull with sockets/jaw, spine, five rib
      pairs, pelvis, arms/hands, legs/feet (1352 tris, 24 joints).
- [x] Standing, sitting-on-floor and sitting-in-chair poses; the chair pose
      targets a 0.45 m seat and is placed against the separate `core:chair`
      with documented offsets (`dz = -0.0192 m`, torso clears the backrest by
      5.9 mm, feet flat on the floor, knees past the seat edge). No chair is
      baked into the model.
- [x] Real rig: root/pelvis, spine/chest, neck/head, shoulders, upper arms,
      elbows, wrists/hands, thighs, knees, ankles, feet/toes; meaningful pivots
      and useful bone names; geometry chain-restricted (no rib follows an arm).
- [x] Engine-path `the_shipped_skeleton_sits_through_its_pose_cues` drives hip
      flexion, knee bending, ankle placement and seated poses through the real
      animator: chair pelvis 0.44–0.45 m, floor-sit pelvis ≤ 0.15 m, soles on
      the floor in every pose; limbs stay connected (bounded edge stretch in
      the offline sweep, ≤ 1.22×).

#### Integration and acceptance

- [x] Catalogued with stable ids (`mannequin`, `rat`, `skeleton`),
      display names, sizes matching the bind-pose bounds, colours, categories
      and descriptions.
- [x] Test fixture (`tests/fixtures/levels/entity_showcase.json`) rather than a
      new bundled demo map: two independently routed rats, a mannequin pose
      cycle, two skeleton pose cycles at a real chair, label interactions.
- [x] Two instances animate independently and stay collision-aware and
      interactable (game + render tests; live anchors follow).
- [x] Independent-reviewer findings fixed with regressions: reset restores the
      authored anchor/bounds, `play_animation` can pose a prop with no
      interaction of its own, a route turn re-orients the live aim bounds, and
      the runtime/validation disc floors are one shared constant (§9.9).
- [x] Focused catalog/asset/pose regressions without total-count assertions;
      Spoonerman untouched except the editor proxy repair described in §9.8.
- [x] Manifest below (§9.3) with runtime/source paths, clips, dimensions and
      zoo-placement requirements.

### 9.3 Asset manifest

| asset (catalog id) | runtime path | source path | size [w,h,d] m | joints | tris / verts | clips (loop, measured reference) | texture |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `mannequin` | `assets/entities/mannequin/model/mannequin.glb` | `tools/entities/build_mannequin.py` + `assets/entities/mannequin/textures/concrete_grey_01.png` | `[0.42, 1.72, 0.305]` | 23 | 1130 / 2450 | `pose_stand`, `pose_arms_up`, `pose_arms_forward` (holds, no speed) | 256² embedded |
| `rat` | `assets/entities/rat/model/rat.glb` | `tools/entities/build_rat.py` + `assets/entities/rat/textures/rat_fur_01.png` | `[0.095, 0.139, 0.613]` | 25 | 856 / 1848 | `idle` 2.4 s loop, `walk` 0.4 s loop @ 0.1985 m/s, `run` 0.24 s loop @ 0.5731 m/s | 256² embedded |
| `skeleton` | `assets/entities/skeleton/model/skeleton.glb` | `tools/entities/build_skeleton.py` + `assets/entities/skeleton/textures/bone_01.png` | `[0.4175, 1.70, 0.2404]` | 24 | 1352 / 2888 | `pose_stand`, `pose_sit_floor`, `pose_sit_chair` (holds) | 256² embedded |
| `spooner-man` (unchanged) | `assets/entities/spooner-man/model/spooner-man.glb` | run-03 `tools/props/animate_spooner_man.py` | `[0.165, 0.389, 0.626]` | 26 | 1380 / 3029 | `idle`, `walk` (0.26 m/s), `sit_down`, `sit_idle`, `stand_up` | 256² embedded |

Zoo/editor requirements: all four are ordinary catalog placeables
(`asset_class/type: entity`, `entity_type: character`, no theme, `solid:
false`), so the later model zoo can place them through the prop system with
the sizes above; `assets/prop_proxies.json`, the editor's built-in catalog
mirror and the 64×64 thumbnails are regenerated
(`tools/entities/build_editor_assets.py`), and `prop_showcase.json` places
each once. Route speeds to author: rat `0.1985` (walk) / `0.5731` (run);
mannequin and skeleton are pose-only. The skeleton's chair placement offset is
in `assets/entities/skeleton/README.md`; the rat's origin sits ~0.12 m behind
the torso centre because the tail dominates its bind box.

### 9.4 Implementation (files and interfaces)

- **`src/entity.rs` (new).** `PoseCue` (moved from the renderer),
  `angle_difference`/`turn_toward`, `EntityRoute`, `EntityRoutes`
  (`from_level`, `get`, `index_of`), `RouteState`
  (`new_state`/`advance`/`enter_step`/`block`), `RouteWorld`, `EntityFrame`.
  Constants: turn 240 deg/s, arrive 0.02 m, face 0.02 rad, substep 1/60 s,
  frame clamp 0.1 s, step height 0.3 m, block epsilon 1 mm.
- **`src/level.rs`.** `EntityRouteDef` (`id`, `loop`, `steps`),
  `RouteStepDef` (serde tag `step`), `LevelDef.routes`,
  `MAX_LEVEL_ROUTES`/`MAX_ROUTE_STEPS`/`MAX_ROUTE_SPEED_MPS`/
  `MAX_ROUTE_WAIT_SECONDS`/`MAX_ROUTE_PLAY_SECONDS`; `ActionDef::PlayAnimation`
  gained `clip` + `loop` and is no longer reserved.
- **`src/loader.rs`.** `validate_routes` + `validate_route_steps` +
  `route_path_is_clear` in `validate_level`; `validate_action` accepts
  `play_animation` (target rule like `toggle_label`, non-blank clip);
  `play_audio` still rejected by name.
- **`src/game.rs`.** `CollisionWorld.routes`; `Game::{routes, route_state,
  entity_frames, animation_override}`; `seed_entity_state`,
  `rebuild_entity_frames`, `update_entities`, `sync_routed_interactables`;
  `DispatchReport.animations_started`; `reset_to_spawn` re-seeds routes.
- **`src/render/common/character.rs`.** `Character` carries `instance_id` and
  `scale` with `set_pose`; `CharacterScene::update(delta, snapshot, frames)`;
  per-clip reference speeds (`clip_reference_speeds`) with
  `RUN_GAIT_MULTIPLIER = 1.5` and `RUN_REFERENCE_SPEED_MPS = 1.2` fallbacks.
- **`src/render/wgpu/character.rs`.** `CharacterGpu.uploaded_transform`; a moved
  character rewrites its environment uniform (`EnvironmentBindings::update`
  with the stored `environment_template`) and bounds; the vertex buffer is
  still written only when the pose revision changed.
- **`src/render/facade.rs`, `src/render/wgpu/renderer.rs`, `src/main.rs`.**
  `update_characters(delta, locomotion, frames)` threaded through; `main`
  passes `game.entity_frames()`.
- **`src/interact.rs`.** `Interactable.size` and
  `Interactables::set_live_pose` for the controller's live-anchor/bounds sync;
  `referenced_targets` now collects `toggle_label` **and** `play_animation`
  targets as cue-only instances.
- **`tools/entities/rig.py` (new).** Pure-Python rigged-GLB writer: joints,
  IBMs, `JOINTS_0`/`WEIGHTS_0`, LINEAR clips, extras metadata, structural
  checker; `auto_weights` nearest-segment blend.
- **`tools/entities/build_{rat,mannequin,skeleton}.py`** (imported from the
  specialists), `tools/entities/validate_entities.py`,
  `tools/entities/render_contact_sheets.py`,
  `tools/entities/build_editor_assets.py`, `tools/entities/README.md`.
- **Maps/data.** `tests/fixtures/levels/entity_showcase.json` (new),
  `assets/catalog.json`, `assets/prop_proxies.json`,
  `level-editor/js/props.js`, `level-editor/assets/thumbs/{rat,mannequin,
  skeleton,spooner-man}.png`, `tests/fixtures/levels/prop_showcase.json`
  (regenerated with the three entities), `tools/levels/build_fixture_levels.py`,
  `tools/assets/validate.py` (routes + implemented `play_animation`).
- **Docs.** `docs/ARCHITECTURE.md` (animation/route/live-anchor contract),
  `docs/RENDERER.md` (cue/live-transform paragraph), `docs/MAP_AUTHORING_GUIDE.md`
  §29 (routes + `play_animation`), the entity READMEs, `assets/README.md`,
  `README`-level changelog.

### 9.5 Verification performed

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean (exit 0) before the final suite |
| Focused `cargo test --bin places -- entity::` | 5 passed |
| Focused `cargo test --bin places -- game::tests::a_route game::tests::play_animation` | 4 passed (route motion + live anchor, wall block, override/reset, unsupported clip) |
| Focused `cargo test --bin places -- loader::tests::test_validate_` routes | 2 passed (accepts a clear route; rejects 10 malformed cases) |
| Focused `cargo test --bin places -- the_shipped_mannequin… / the_shipped_rat… / the_shipped_skeleton… / two_placed_characters…` | 4 passed (real animator pose selection, rat gait contact, skeleton sits, two-instance isolation) |
| `cargo test --workspace --no-fail-fast` (final) | see §9.7 |
| `python3 tools/entities/validate_entities.py --workers 1` | 19 chunks, 0 problems; rat walk/run contact and reference speeds re-measured |
| `python3 tools/entities/rig.py --check` on all three GLBs | 0 problems each |
| `python3 tools/assets/validate.py` | 124 assets (36 placeable), 0 errors / 0 warnings over demo + Pit + fixtures |
| `python3 tools/props/build.py --check` | 35 models parse; mannequin/rat/skeleton report their real tri/vert counts |
| `python3 tools/textures/build.py --check` | 46 textures, only the pre-existing 1024×1024 surface warnings |
| `cd level-editor && npm test` | 144/144 (including the repaired spooner-man proxy bounds and the catalog mirror) |
| `python3 tools/entities/render_contact_sheets.py --workers 3` | rat 27 cells, mannequin 9, skeleton 9; front/side/three-quarter sheets reviewed under `target/entity-verify/` |
| `PLACES_CAPTURE` / interactive GPU run | **not possible** (headless environment); see §9.8 |
| `python3 -m unittest tests.test_package` | pre-existing failure: `icon.png` is 1254 px against the 512 px test limit (file untouched by this run); everything else passes |

The Blender sheets were inspected: the rat's side views show stepping legs,
planted paws and a streaming tail in the run; the mannequin's three poses read
from all three angles with the concrete sheet; the skeleton's standing, floor
and chair sits read with grounded feet and a readable skull/rib/pelvis
silhouette. This is offline reimport evidence, not engine playback.

### 9.6 Offline compute (multicore requirement)

Two new tools do the expensive offline work; both are bounded, deterministic
and report requested vs effective workers.

**`tools/entities/validate_entities.py`** — per-frame skinning, contact and
deformation sweep over the built GLBs. Bottleneck: pure-Python per-vertex
4-influence matrix blending plus per-edge stretch, one independent task per
(asset, clip, 16-frame chunk). Backend: `multiprocessing` with the explicit
`spawn` context, a module-level worker function, per-process GLB parsing in an
initializer, and a deterministic parent-side merge keyed by task index.
`--workers N` (CLI wins) / `PLACES_TOOL_WORKERS`, `min(12, usable CPUs)` with
`os.process_cpu_count()` first, ~64 MiB/worker memory guard and a reduction
when tasks < workers.

| part | measurement |
| --- | --- |
| Full sweep, serial (`--workers 1`) | 19 tasks, 0.47 s internal / 0.56 s wall |
| Full sweep, `--workers 4` | 0.24 s internal / 0.35 s wall |
| Full sweep, `--workers 8` | 0.19 s internal / 0.29 s wall |
| Full sweep, `--workers 12` | 0.20 s internal / 0.30 s wall (12 > 10 tasks, so capped by work) |
| Serial and parallel outputs | identical merged per-clip numbers (checked against the JSON output) |

The algorithmic reduction is the point: the sweep validates only the *written
bytes* and only the sampled frame grid (default 60 Hz, single frame for a
zero-duration hold), rather than re-solving any rig search. There was no slow
rig fitting, pose search or weight solve to parallelise — the assets are
directly constructed, single-process, deterministic builds (`build_*.py`); the
sweep is the one genuinely independent workload.

**`tools/entities/render_contact_sheets.py`** — reimports each GLB into an
isolated Blender process (Cycles CPU, pinned to 2 native threads) and renders
front/side/three-quarter cells; the parent assembles the sheets. Workers are
asset-level and bounded by `min(12, CPUs)` and the asset count.

| part | measurement |
| --- | --- |
| Contact sheets, serial (`--workers 1`) | 3 assets / 45 cells, 25.6 s internal / 32.0 s wall |
| `--workers 2` | 15.25 s internal / 21.6 s wall (≈1.7×) |
| `--workers 3` | 15.18 s internal / 21.5 s wall (rat dominates, so 3 ≈ 2) |
| `--workers 12` | 14.90 s internal; effective 3 (only three independent assets), same wall as 3 |

Blender is only a verification dependency (not part of the asset toolchain);
each Blender process writes only into its own scratch directory and the parent
alone writes the final sheets, so the single-writer rule holds.

Progress and recovery: both tools print the resolved worker configuration,
per-asset/per-clip completion and the merged totals with flushed stdout;
neither needs a 5–15 s heartbeat because the measured jobs are 0.4–1 s (sweep)
and ~7 s per asset (Blender), and the per-asset lines already land inside that
band for a larger set. There is no checkpoint/resume: the sweep is a short
bounded linear pass over the written bytes (re-running it costs under a
second), not a resumable search. Worker exceptions propagate (`imap_unordered`
surfaces them; Blender exit status and a missing-cell check fail the asset),
so incomplete work is reported incomplete and the tool exits non-zero.

Reductions are explicit: a requested `--workers 99` reports
`requested 99, reduced to 12 (CPU/ceiling budget)`; the sweep additionally caps
by independent tasks and a per-worker memory guard (a worker holds one decoded
model plus scratch; `MEMORY_GUARD_BYTES` = 64 MiB); the Blender tool explains
`worker count reduced to N: only M independent asset(s)`. The worker-reduction
paths were re-run after these guards landed, and the parallel sweep output
still matches the serial reference exactly.

The asset builds themselves are deliberately serial: each `build_*.py` is a
direct low-poly construction (boxes/tubes/lathes with a deterministic
nearest-segment weight blend), not a solver, so there is no expensive
generation stage to parallelise; parallelising a 0.2 s build would only add
startup. The multicore requirement is applied where real independent CPU work
exists: the offline sweep and the Blender render set.

### 9.7 Final workspace checks

All checks below were run on the final tree (after the reviewer-driven fixes).

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean (0 warnings, exit 0) |
| `cargo test --workspace --no-fail-fast` (final) | **1225 passed, 0 failed, 9 ignored** (423.47 s) |
| New run-04 regressions | `a_route_moves_the_entity_and_its_live_anchor_follows`, `a_route_never_walks_the_entity_through_a_wall`, `play_animation_sets_a_pose_override_that_reset_clears`, `a_programmatic_animation_without_a_clip_is_unsupported_not_a_panic`, `the_entity_showcase_runs_two_independent_rat_routes`, `reset_to_spawn_restores_a_routed_entitys_authored_anchor_and_bounds`, `play_animation_can_pose_a_prop_with_no_interaction_of_its_own`, `a_route_turn_reorients_the_live_aim_bounds`, `validate_routes_uses_the_runtime_minimum_disc_radius`, `a_one_shot_route_holds_a_final_play_clip_but_settles_after_a_walk`, `two_placed_characters_follow_independent_entity_frames`, `the_shipped_mannequin_selects_its_poses_through_cues`, `the_shipped_rat_walks_and_runs_with_declared_reference_speeds`, `the_shipped_skeleton_sits_through_its_pose_cues`, `a_missing_or_malformed_clip_marker_keeps_the_clip_defaults`, plus the loader route-validation, cue-resolution and marker tests |
| `python3 tools/assets/validate.py` | 124 assets (36 placeable), 0 errors / 0 warnings |
| `python3 tools/props/build.py --check` | 35 models parse; entity tri/vert/texture counts within budget |
| `python3 tools/textures/build.py --check` | 46 textures; only the pre-existing preferred-size warnings |
| `cd level-editor && npm test` | 144 passed, 0 failed |
| `python3 tools/entities/validate_entities.py --workers 1 / 4 / 8 / 12` | 19 tasks, 0 problems each; merged per-clip numbers identical |
| `python3 tools/entities/render_contact_sheets.py --workers 1 / 3` | all cells rendered; sheets inspected |
| `python3 -m unittest tests.test_package` | pre-existing `icon.png` 1254 px > 512 px failure only (file untouched by this run); other 41 tests pass |

Baseline before this run: run 03 finished at 1202 passed / 9 ignored (from
§8.6); the final 1225 includes the run-04 tests and the reviewer regressions
and regresses none of the earlier ones. The full-suite count also depends on
generated drop-in levels (`levels/level0_pit.json` is gitignored); it was
present here.

### 9.8 Remaining issues / next-run dependencies

- **No GPU/engine playback capture.** Still headless: `PLACES_CAPTURE`, an
  interactive `E` press and a timed engine playback of the routes were not
  possible. The route/pose integration is covered by deterministic controller
  tests plus the real `CharacterScene`/`CharacterAnimator` CPU path, and the
  visual evidence is the Blender reimport sheets; a hardware validation run
  should capture the two rats walking/running, the mannequin cycle and the
  seated skeleton.
- **Route `play` steps use explicit seconds.** A one-shot clip's completion is
  not fed back from the renderer; a route step holds its cue for the authored
  `seconds` instead. A future run that wants clip-exact sequencing can plumb
  `take_cue_finished()` back through `update_characters` keyed by instance id.
- **Routed entities respect walls/floor but are not player colliders.** The
  runtime disc uses the narrower footprint axis and never pushes the player;
  the fixture's entities are `solid: false`. A future run wanting bumpable
  creatures needs a moving collider or a character-vs-character query.
- **Entity lighting is sampled at spawn.** A long route carries the spawn's
  baked light; the character path already behaves this way for Spoonerman.
- **Route validation does not know catalog models.** It validates against the
  placed `size` and the level geometry; a route naming a prop whose model has
  no matching clip will fall back to clip 0 (logged nowhere yet).
- **`play_animation` clip existence is a runtime lookup.** Validation cannot
  know an asset's clips from a level; an unknown clip name plays clip 0 by
  design (the animator's existing fallback).
- **Editor proxy fidelity.** The entity editor proxies are derived three-box
  decompositions (tooling-generated), not art; the runtime is unaffected.
- **Pre-existing repository failures recorded, not introduced:** the 1254 px
  `icon.png` against `tests/test_package.py`'s 512 px limit. The stale
  spooner-man editor proxy (0.272 m against the 0.165 m catalog box) *was*
  repaired this run because the editor consistency tests otherwise fail once
  the catalog grows.

### 9.9 Independent review

The independent asset/playback reviewer worked on a pinned copy of the tree
under `target/reviewer-04/`, re-derived the asset numbers from the GLB bytes
with its own Python, wrote throwaway probe tests (including a full workspace
copy under `target/reviewer-04/repro/`) and re-ran the tools. It confirmed the
writer fixes (`COLOR_0` `normalized: true`, `Rig.segment` starting at the joint
origin), the no-tunnelling behaviour at 6 m/s with clamped frames, the
validation rejections, per-instance overrides, the Spoonerman marker fallback
and the serial/parallel sweep equivalence. Four real defects were found and
**fixed in this run**:

1. **`reset_to_spawn` kept the live anchor/bounds** (medium). `seed_entity_state`
   re-captured the "authored" rest values from the already-live
   `Interactable`s, so after a reset a routed entity's label and aim box stayed
   where it was when the reset fired. Fixed structurally: the live values are
   now recomputed from the route state and the instance's own `size` every
   frame (`Interactables::set_live_pose`), so a reset (which re-seeds the route
   at its spawn) restores the authored values by construction and route turns
   also re-orient the bounds. Regression:
   `reset_to_spawn_restores_a_routed_entitys_authored_anchor_and_bounds`.
2. **A `play_animation` target without its own interaction validated but could
   not resolve at runtime** (medium). `Interactables::from_level` only included
   props with actions or `toggle_label` targets, while validation accepted any
   placed-prop id. Fixed: the referenced-target set now includes
   `play_animation` targets as cue-only instances (never aimable), matching the
   documented label rule. Regression:
   `play_animation_can_pose_a_prop_with_no_interaction_of_its_own`.
3. **Live aim bounds translated but never re-oriented on a route turn**
   (low-medium). The bounds were baked from the spawn yaw. Fixed by the same
   `set_live_pose` change; regression `a_route_turn_reorients_the_live_aim_bounds`
   asserts the half-extents swap rather than grow through a 90° face step.
4. **Runtime disc floor (0.08 m) exceeded the validator's (0.05 m) for tiny
   props** (low; a stall, never a tunnel). Fixed with one shared
   `entity::ENTITY_MIN_RADIUS_M` used by both; regression
   `validate_routes_uses_the_runtime_minimum_disc_radius`.

Reviewer notes also addressed: an unknown clip name now logs once and falls
back to the first clip (`update_cued`); the unused `MEMORY_GUARD_BYTES` is now
a real per-worker cap and the worker-count docs match; the Blender tool
explains its asset-count clamp; the leftover temporary selftest test was
removed; the `PlayAnimation` doc no longer implies a route step clears an
override; `WgpuCharacters` now stores each character's scene slot instead of
assuming GPU and scene indices are identical; and a malformed/missing
`places_entity_clips` marker has a dedicated regression
(`a_missing_or_malformed_clip_marker_keeps_the_clip_defaults`).

Noted but deliberately unchanged: an unknown clip still falls back to clip 0
(now logged); the runtime disc remains the narrower footprint axis; routed
entities remain non-solid and do not push the player.

## 10. Run 05 — general props, luminous signs, the toggle action, floating props and the editor removal

Run 05 added the remaining requested prop models and integrated them, added one
map-authored interaction action, one rigid-animation runtime, one floating-prop
component, and removed the retired level editor completely. It carries forward
every run-01..04 change.

### 10.1 Prerequisites repaired

* **Multi-material GLBs shared their whole vertex list with every primitive.**
  The toolkit's extended writer referenced one POSITION/TEXCOORD_0/COLOR_0
  accessor from every primitive, so the runtime's per-primitive expansion
  (`src/gltf.rs::assemble_primitive`) repeated the whole vertex list for each
  material run: `core:exit_sign` 256 vertices for 168 submitted, `home:ball_light`
  884/594, `home:wall_switch` 200/150, which broke
  `non_indexed_submission_duplicates_prop_vertices`. `tools/props/glb.py`
  `_write_extended_glb` now emits per-primitive compacted attribute arrays
  (only the vertices that primitive's indices reference, in first-use order),
  so the invariant `unique <= submitted` holds and the GLBs shrink. The three
  models were rebuilt; the legacy single-primitive writer is untouched and
  byte-identical.
* **A floating prop leaked a placeholder box into the asset-less build path.**
  `src/render/common/api.rs::build_level_geometry_with_catalog_and_materials`
  passed every prop as a fallback box, so the vertex-lit parity builds disagreed
  by the duck's 24-vertex box while the asset-aware path skipped it. The
  asset-less path now skips `prop.float` exactly like `resolve_prop_instances`,
  and the demo's animated-prop expectation was updated for the second animated
  prop (the rigid switch).

### 10.2 Assets implemented

Twelve models, all registered in `assets/catalog.json` with stable ids, display
names, sizes, colours, categories and solid flags, built by the toolkit, and
placed in the fixtures (and, for the run's demonstrations, in Places Demo):

| id | model (under `assets/`) | tris | notes |
| --- | --- | --- | --- |
| `core:stop_sign` | `core/props/models/stop_sign.glb` | 80 | metal pole + red octagon + white STOP block text |
| `core:exit_sign` | `core/props/models/exit_sign.glb` | 56 | green face + white EXIT + arrow; emissive face material; prop light in the demo |
| `home:ball_light` | `environment/home/props/models/ball_light.glb` | 198 | straight cord + ceiling rose + emissive orb; prop point light |
| `home:wall_switch` | `environment/home/props/models/wall_switch.glb` | 50 | plate + hinged rocker; rigid node clip `toggle` (no skin) |
| `home:crt_tv` | `environment/home/props/models/crt_tv.glb` | 172 | period cabinet, curved screen, knobs |
| `home:knife` / `home:fork` / `home:spoon` | `environment/home/props/models/*.glb` | 34 / 84 / 72 | table scale, +Z = working end |
| `home:plate` / `home:bowl` | `environment/home/props/models/*.glb` | 192 / 168 | base-centre origin |
| `home:plant_table` | `environment/home/props/models/plant_table.glb` | 286 | small potted table plant |
| `core:rubber_duck` | `environment/pool/props/models/rubber_duck.glb` | 208 | yellow hull/head/beak/eyes; floats in the demo pool |

Every model is ≤500 triangles, base at `y = 0`, centred, one embedded texture
(64–256 px), UVs inside `0..1`. Source PNGs ship beside the domestic and pool
models; the signs, lamp, switch and CRT are painted in their builders.
`tools/props/glyphs.py` is the shared 5×7 block font used by both the prop
signs and the decal painter (the decal rebuild is byte-identical).

Registered metadata and the per-model derived numbers are in
`target/agent-work/run05-asset-manifest.md`; preview sheets and engine-view sign
renders are in `target/agent-work/run05-previews/` and
`target/agent-work/sign-checks/`.

### 10.3 Runtime interfaces

* **`toggle_animation`** (`src/level.rs::ActionDef::ToggleAnimation`,
  `target` optional, `clip` required) — validated in `src/loader.rs`
  (missing/blank target/clip and unknown instance are named errors), included in
  `src/interact.rs::referenced_targets`, and dispatched by
  `Game::stage_toggle_animation` (`src/game.rs`). It flips the acting instance's
  own scrub target between `t = 0` and `t = duration` and leaves every other
  instance alone; `reset_to_spawn` re-aims toggles to their rest end while still
  clearing `play_animation` overrides (`Game::seed_entity_state`).
* **`PoseCue::Scrub { name, target }`** (`src/entity.rs`) — the animator eases
  the clip time toward `target * duration` at a constant clip-time rate
  (`SCRUB_TRAVERSE_SECONDS = 0.35` s per full traverse, any authored clip
  length), so a second press mid-move reverses from the current pose, holds
  either end, and reports each arrival once
  (`src/render/common/character.rs::update_cued`). Covered by
  `a_scrub_cue_reverses_from_its_current_pose_and_holds_its_ends`.
* **Rigid animated props** — `src/gltf.rs` retains the node hierarchy for a
  model that declares clips but no `skins`, binds every primitive's vertices to
  its owning node with weight one, and `PropModel::{is_animated, is_animatable}`
  plus `Rig::new_rigid` let the character path pose it
  (`CharacterAnimator::is_rigid`). A rigid animator never runs the locomotion
  driver: with no cue it holds its bind pose
  (`an_idle_rigid_prop_holds_its_bind_pose`), and a level's animated-prop
  budget is `MAX_CHARACTERS = 8`.
* **Floating props** — `PropDef.float` (`PropFloatDef {draft, bob, bob_seconds,
  heel_degrees, heel_seconds, phase}`, `src/level.rs`), validated by
  `src/loader.rs::validate_floats` (solid, size, bounds, route and
  water-containment named errors) and mirrored in `tools/assets/validate.py`.
  `WaterVolumes::contains_disc` proves the swept footprint fits one volume. The
  prop is skipped by the static batch (`resolve_prop_instances`) and by the bake
  occluders (`prop_is_static`), and is spawned and advanced by
  `DynamicScene::{spawn_floating_props, update_floats, clear_floats}` from an
  absolute clock (`y = surface - draft + bob·sin(...)`, heel about the model's
  local Z, x/z never move), wired through
  `WgpuRenderer::{update_dynamic, set_floating_props}` and
  `main.rs::spawn_level_demonstration` (idempotent per level load).
* **Luminous props** — the exit sign owns a green `rect` light in front of its
  face and the ball light a warm `point` light 2 cm below its orb (both
  authored in the demo's `props[].lights`). The materials' emission is separate
  and never lights a room; `the_demo_exit_sign_and_ball_light_really_illuminate`
  samples the bake with the lights enabled and disabled to prove the room
  actually gains green/warm light.

### 10.4 Level integration (Places Demo, additive only)

`kitchen_switch` (reachable, on the kitchen-opening column: `toggle_animation` +
`toggle_label` in one batch), `kitchen_pendant` (over the existing table),
`corridor_exit_sign` (east corridor ceiling), `kitchen_{plate,fork,knife}_{west,
east}` at the real tabletop (`y = 0.75`, room `floor_y = -0.9`), and `pool_duck`
(`float`, 0.4 m off the north rim). The fixtures place every one of the twelve
once (`tests/fixtures/levels/prop_showcase.json`, regenerated by
`tools/levels/build_fixture_levels.py`). No existing room, wall, prop or light
was moved.

### 10.5 Level editor removal

Removed completely; the repository is editor-free.

* Deleted: `level-editor/` (shell, 13 JS modules, vendored JSZip, 10 Node
  suites, generated thumbnails, `package.json`, its README), `src/lighting_parity.rs`,
  `tools/entities/build_editor_assets.py`, `assets/prop_proxies.json`.
* Removed editor-only machinery: `tools/props/build.py` (`PROXY_PATH`,
  `THUMB_DIR`, `write_proxies`, `--no-proxies`, `--thumbs`, `parts` report),
  `tools/props/preview.py` (`render_thumbnails`, `--thumbs`), the `Mesh.parts`
  proxy metadata and every `proxy=` argument across the prop and entity
  builders, `tools/props/generate_spooner_man.py`'s proxy/thumbnail steps,
  `tools/verify.sh`'s Node gate, and the editor comment/doc references in
  `src/`, READMEs, `assets/README.md`, `docs/ARCHITECTURE.md`,
  `docs/RENDERER.md`, `docs/VERIFICATION.md`, `docs/MAP_AUTHORING_GUIDE.md` and
  `docs/ASSET_SPECIFICATION.md` (with the §12 renumbering).
* Preserved: the game, renderer, level format, levels, catalog, models,
  textures, fixtures, GLB/level/texture validation, the Python prop/entity/
  texture/level toolkits (headless authoring), `tools/entities/build_*.py`,
  `rig.py`, `validate_entities.py`, `render_contact_sheets.py`, and the
  authoring documentation. Nothing needed relocation: no file outside
  `level-editor/` consumed anything inside it.
* Historical records (`CHANGELOG.md` entries, the run 01–04 sections of this
  report, `docs/renderer-baseline/BASELINE.md`) intentionally keep the word
  "editor". A new `CHANGELOG.md` "Unreleased — retire the legacy level editor"
  section records the removal; no replacement is promised.

### 10.6 Checks and results

| Check | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | exit 0 (the tree carried pre-existing drift from earlier runs; it is formatted now) |
| Lint (strict) | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0, no warnings |
| Tests (final tree) | `cargo test --workspace --no-fail-fast` | exit 0; see the final-suite line below |
| Prop pack | `python3 tools/props/build.py --check` | exit 0, 48 props, no FAIL |
| Prop rebuild | `python3 tools/props/build.py --only core:stop_sign core:exit_sign home:ball_light home:wall_switch home:crt_tv home:knife home:fork home:spoon home:plate home:bowl home:plant_table core:rubber_duck` | exit 0, 12 props, 1600 triangles, 277 KiB |
| Textures | `python3 tools/textures/build.py --check` | OK (46 textures, only the pre-existing preferred-size warnings) |
| Assets + levels | `python3 tools/assets/validate.py` | OK (136 assets / 48 placeable, 13 levels, 0 warnings) |
| Entity toolkit (shared authoring preserved) | `python3 tools/entities/validate_entities.py --workers 4`; `python3 tools/entities/build_rat.py --out /tmp/rat_check.glb`; `python3 tools/entities/rig.py --check /tmp/rat_check.glb` | 0 problems; the rat builds and checks after the `proxy=` removal |
| Fixtures | `python3 tools/levels/build_fixture_levels.py` | regenerated `prop_showcase` (39 props, all 12 new) and `prop_stress` |
| Signs in the engine's front view | own orthographic +Z render of both GLBs | `STOP` and `EXIT →` read left-to-right (the toolkit preview rasteriser mirrors horizontally; do not use it for text) |
| Previews / manifest | `python3 tools/props/preview.py --only <12> --sheet --out target/agent-work/run05-previews` | sheets 1–2 rendered; manifest at `target/agent-work/run05-asset-manifest.md` |
| Editor removal search | `rg -n "level-editor\|level_editor\|prop_proxies\|lighting_parity\|build_editor_assets" src/ tools/ tests/ assets/ docs/ README.md` | no live hit (historical records only) |
| Packaging | `python3 -m unittest tests.test_package` | one pre-existing failure: `icon.png` is 1254 px against the 512 px test limit (file untouched; recorded in §9.7 too) |
| GPU capture / interactive `E` | `PLACES_CAPTURE`, an interactive press | **not possible** in this headless environment; see §10.8 |

`cargo test --workspace --no-fail-fast` (**final tree**):

`1246 passed, 0 failed, 8 ignored` (453.7 s, exit 0), including the new Run 05
regressions: `an_animated_unskinned_prop_parses_as_a_rigid_animated_model`,
`a_scrub_cue_reverses_from_the_current_pose_and_holds_its_ends`,
`an_idle_rigid_prop_holds_its_bind_pose`,
`toggle_animation_flips_one_instance_and_composes_with_a_label`,
`toggle_animation_rejects_a_missing_clip_or_target`,
`reset_retargets_toggles_to_rest_and_clears_playing_animations`,
`the_demo_exit_sign_and_ball_light_really_illuminate` (soft and hard bakes,
lights on vs disabled, emitter side checks), and the seven floating-prop
blocks (serde defaults, `contains_disc`, validator accept/reject,
dynamic spawn/envelope/phase/idempotence, static-path skip, no collider, no
character claim).

Two Run 05 defects were found by these checks and fixed in the run: the
multi-primitive GLB writer repeated the shared vertex list per primitive
(compacted now), and the asset-less geometry path emitted a placeholder box for
a floating prop (skipped now). A third, found by the run's own scrub test: a
re-targeted scrub kept its "arrived" flag, so a lever toggled after reaching an
end reported no further arrival (`update_cued` now clears it while in flight).

### 10.7 Offline compute and multicore work

No new expensive solver was added: the asset generators are deterministic
box/lathe/tube constructions (the whole 12-model rebuild above is ~1 s serial),
so parallelising them would only add process startup. The heavy offline work
this run must rerun is already parallel from earlier runs and was re-measured:

* `python3 tools/entities/validate_entities.py --workers 1` vs `--workers 12`
  — measured on the final tree: 0.58 s serial → 0.31 s (1.9×) over the same 19
  tasks, identical merged numbers, 0 problems in both; 12 workers is the
  fastest safe configuration for this corpus and the tool prints its resolved
  worker count (and its reduction message for an over-requested count).
* `python3 tools/entities/render_contact_sheets.py --workers 1/3` — Blender
  renders are the only genuinely expensive offline stage; each worker writes
  its own scratch cells and the parent alone writes the final sheets.

Nothing new was parallelised and nothing already-parallel was changed; the
requirement is satisfied by not adding serial brute force and by reusing the
existing process-based worker budget.

### 10.8 Remaining issues / next-run dependencies

* **No GPU/engine capture.** Still headless: `PLACES_CAPTURE` and an
  interactive `E` press were not possible. The switch, the two lights, the
  duck's float, the table settings and the sign text are covered by
  deterministic CPU tests (animator scrub, dispatch, bake samples, dynamic
  poses, orthographic renders) plus the toolkit previews; a hardware run should
  capture the lever flipping, the green/warm pools, the duck bobbing and the
  place settings.
* **`reset_to_start` retargets toggles, it does not re-play them.** The scrub
  eases back to `t = 0` at 0.35 s per traverse; a level that wants an instant
  reset will need a new cue semantic.
* **Rigid props count against the animated-character budget**
  (`MAX_CHARACTERS = 8`). A level with more than eight animated props (skinned
  or rigid) leaves the extras in their bind pose; the warning names the budget.
* **The prop toolkit cannot build entity placeables.** `python3
  tools/props/build.py` with no flags still aborts on `mannequin`/`rat`/
  `skeleton` (pre-existing; entities are built by `tools/entities`). Use
  `--only <ids>` or `--check`.
* **Stale shipped GLBs/PNGs remain** for older models (pre-existing, recorded
  in §9.8/§9.9 and confirmed at HEAD by the reviewer): rebuilding some older
  models with the current painters changes their texture bytes. The run-05
  models are reproducible byte-for-byte.
* **The editor is gone, so `prop_proxies.json` and editor thumbnails no longer
  exist.** Any external workflow that read them must be updated; no in-repo
  consumer remains.
* **Five new models are demonstrated in the fixtures only** (`core:stop_sign`,
  `home:crt_tv`, `home:knife`/`home:fork`/`home:spoon` beyond the two place
  settings): the run placed every *light* prop, the switch, the duck and the
  full table settings in the demo as requested and deliberately did not
  redesign the demo further.
* **Pre-existing packaging failure:** `tests/test_package.py` still fails on
  `icon.png` (1254 px > 512 px); unrelated to this run and untouched.

### 10.9 Independent review

The independent interaction/visual reviewer worked on a pinned copy
(`target/reviewer-05/repo`) and re-derived every claim from the GLB bytes, the
level JSON and the code, writing its own probes and Rust tests. Its report is
`target/agent-work/reviewer-05.md`. It confirmed: the editor removal is
complete and broke no shared authoring; all twelve assets are real (triangle
counts, bounds vs catalog size, base/centring, UV bounds, embedded PNGs,
emissive factors, the switch's `toggle` clip and its exact
`T(p)·R(t)·T(-p)` hinge); sign text and arrow read correctly on the +Z faces;
switch semantics (mid-travel reversal, independent instances, reset
re-targeting, validator agreement); the duck's float (rest pose, envelope,
containment, no collider/batch/character, idempotent replays); real
illumination from both props and the table settings' exact tabletop contact.

It found four working-tree regressions (the multi-primitive vertex sharing, the
demo's animated-prop count, and the float's placeholder box in both vertex-lit
parity checks) — all four are fixed in this run's final tree, and the fixes
were re-verified. It also independently recorded the pre-existing red gates
above. It could not verify GPU-rendered output, the release build or the
compiled-build/wgpu-bootstrap suites; those remain next-run items.

## 11. Run 06 — curved geometry, visual repairs and the map geometry checker

Run 06 carried forward every run-01..05 change and added reusable curved
architecture, three visual repairs (stair handrails, pool tile sheen, the
automatic-baseboard corner ends), the ceiling-vent decal with grid snapping,
and a runnable read-only map geometry checker with fixtures. No worktrees,
branch switches, resets or commits were used; the working tree is unchanged
apart from this run's edits.

### 11.1 Starting state and roles

Two read-only investigators were run before any edit, and their findings drove
the implementation:

* **Structural-geometry investigator** mapped the wall/floor/ceiling schema,
  the loader validation order and error style, the geometry emitter entry
  points, the collision derivation (`LevelCollision::collision_aabbs`,
  wall-slice decomposition, architecture solids), the lightmap stamping
  requirements, the fixture/audit surface, and the ceiling/decal pipelines.
  It confirmed there was no arc/round primitive anywhere and that no room
  rotation exists in the schema.
* **Materials/visual-defect investigator** reproduced the railing glitch from
  the shipped data and the engine maths (the stair handrails were authored
  2.5 m long from the first tread's facing edge to the top tread's far edge,
  so their endpoint-sampled base line was shallower than the nosing line; the
  posts floated up to 9.5 cm mid-flight and sank up to 7.4 cm near the head)
  and found the doubled end post (posts at 2.4 m and 2.5 m), the sloped-rail
  UV stretch, the pool tile values and the fact that the local `settings.json`
  runs Low, which gates the whole surface response off.

The lead applied every repository edit. An independent validator/false-positive
reviewer was run on a pinned copy after integration (§11.11).

### 11.2 Round walls and circular pillars (delivered)

New data-authored primitives, no hardcoded materials:

* `src/level.rs`: `ArcWallDef` and `PillarDef` with `round_point`,
  `round_segments_for`, `ARC_COLLISION_STEPS`, the `arc_walls`/`pillars`
  arrays on `LevelDef` (empty arrays are skipped in serialization, so existing
  level content keys are unchanged), and the shared interpretation methods:
  `inner_radius`/`outer_radius`/`resolved_segments`/`base_y`/`top_y_at`/
  `collision_boxes` (arc) and `resolved_segments`/`polygon_points`/`base_y`/
  `top_y`/`collision_boxes` (pillar).
* Schema: arc wall `x`, `z` (circle centre), `radius`, `thickness` (default
  0.3, must be `< 2 × radius`), `y` (default the walkable floor under the
  mid-span centreline), `height` (default the local ceiling, per segment),
  `start_degrees`, `sweep_degrees` (default 90; `±360` is a full ring with no
  ends), `segments` (3–128; default 24 per full circle scaled to the sweep),
  `material` plus `inner_material`/`outer_material`/`cap_material`/
  `end_material` and their shine overrides. Pillar: `x`, `z`, `radius`, `y`,
  `height`, `segments`, `material`, `cap_material` and shines. Any catalog
  material id is accepted; the resolved material scan
  (`materials::resolve::push_architecture_materials`) now includes every new
  field, and `architecture_audit::test_every_material_bearing_field_is_discovered`
  pins that.
* Geometry (`src/render/common/architecture.rs::emit_arc_wall`/`emit_pillar`):
  one quad per segment with `orient`-normalised winding, radial per-segment
  normals, top/bottom ring caps (the top cap is skipped when it meets the
  ceiling plane exactly, including at an authored ceiling height), radial end
  caps for a non-full ring (facing along the run's tangent with the sweep's own
  sign, so negative sweeps close outward too — an independent reviewer found
  the first cut mirrored the negative-sweep ends; fixed and pinned by
  `architecture_audit::test_arc_end_caps_face_outward_for_both_sweep_signs`),
  lightmap patches for every quad, and world-scale UVs — `u` is the arc length along each face's own circumference (inner and
  outer faces use their own radius), `v` is height, caps use the world plan
  mapping. A full ring closes without a seam cap. Collision for an arc wall is
  `ARC_COLLISION_STEPS = 4` AABBs per rendered segment, and for a pillar the
  rendered polygon's horizontal rows split at their midpoints; both come from
  the same radii/base/top the emitter uses, and neither is one rectangle
  around the whole circle.
* Validation (`src/loader.rs::validate_arc_walls`/`validate_pillars`): named
  errors for a non-positive radius, thickness at or above twice the radius, a
  zero/over-full sweep, an out-of-range segment count, a non-positive authored
  height, a non-finite base and blank material ids; `validate_surface_shine`
  covers every new shine field. `LevelDef::architecture_estimate` budgets the
  round pieces.
* Limits: `MAX_LEVEL_ARC_WALLS = 1000`, `MAX_LEVEL_PILLARS = 2000`,
  `ROUND_SEGMENTS_MIN/MAX = 3/128`.
* Decal limitation (documented): decals are planar quads and are not supported
  on curved faces; curved surfaces use their own per-face materials.

### 11.3 Wood railing repair (delivered)

Four culprits, three repaired in the engine and one in the map:

1. **Map — the run was too long.** Both stair handrails in
   `assets/levels/places_demo.json` ran 2.5 m (first tread facing edge to the
   top tread's far edge). They now run `2.1875 m`: the first nosing to the last
   nosing, so the endpoint-resolved base line *is* the nosing line
   (slope 0.84) instead of a shallower 0.735 line. The posts stay planted and
   the rail holds 0.95 m above the nosings.
2. **Engine — doubled end post.** `emit_guardrail` now resolves all post
   positions before emitting: when the run's remainder is shorter than a
   quarter bay (floored at two post widths), the last regular post moves onto
   the run's end instead of a second post being added ~10 cm behind it.
3. **Engine — sloped-rail UVs.** `emit_rail_run` tiles `u` along the run's
   true 3D length (`along × √(1 + slope²)`, removing the ~20% stretch on the
   stair rails) and passes the top face its real tilted normal instead of
   `[0, 1, 0]`.
4. **Engine — automatic-baseboard corner ends.** `emit_baseboard` skips an end
   face that would be flush against a perpendicular wall's face
   (`baseboard_end_flush_with_wall`), removing the coplanar wall/trim overlaps
   the new checker found at inside corners. Collision is untouched (trim has
   none; the guardrail barrier box is unchanged). The same pass raises the
   cap-piece and fan-triangle sliver thresholds from 1 mm² to 10 mm², which
   removes two near-degenerate cap triangles the checker found in
   `test_room`'s automatic baseboards; the smallest real corner trim on an
   18 mm board is tens of square millimetres, so no visible trim is lost.

Evidence: `docs/screenshots/run06-stairs-rails-before.png`,
`run06-stairs-rails-after.png` and `run06-stairs-rails-diff.png` (the red diff
traces both handrails and their baked shadows across the flight). A regression
test pins the demo's rails to the stair pitch
(`architecture_audit::test_the_demo_stair_handrails_follow_the_nosing_line`).

### 11.4 Ceiling vent and the room ceiling tile frame (delivered)

* New decal sheet `core:decal_ceiling_vent_01`
  (`assets/core/decals/ceiling_vent_01.png`, 128×128 RGBA cut-out) painted by
  `tools/textures/decal_art.py::build_ceiling_vent`: beveled plate, recessed
  slats, corner fixings; registered in the catalog as a `decal`.
* `decals[].align: "ceiling_grid"` (`DecalAlign`) snaps a ceiling decal's
  centre to the nearest tile/panel centre of the ceiling material above it, in
  the room's own ceiling tile frame, and composes the frame's rotation into
  the decal's in-plane rotation (`LevelDef::snap_ceiling_decals`,
  `ceiling_decal_rotation`, `ceiling_grid_decal_centre`). The snap runs in
  the existing preparation pass; the rotation composition happens at emission
  (`decal_quad_points_rotated`), so the pass is idempotent and the authored
  file keeps its intent. The period is the material's `grid_metres` (the
  visible panel module), falling back to `tile_metres`.
* Per-room ceiling tile frames: `ceiling_tile_origin` (`[x, z]`) and
  `ceiling_tile_rotation_degrees` on `RoomDef`, honoured by the ceiling UV
  emitter, the light-fixture grid snap and the decal snap
  (`RoomDef::ceiling_tile_local`/`ceiling_tile_world`). Absent fields keep the
  world-origin pattern exactly.
* Demo placements: five aligned vents (two office panels, the corridor, the
  pool hall and the home corridor), one authored deliberately off-grid to
  demonstrate snapping. The fixture `geometry_intentional` places two more: one
  authored off-grid on the rotated/offset room frame, one authored exactly on a
  frame lattice point.
* Tests: `level::tests::test_ceiling_tile_frame_round_trips_and_rotates`,
  `test_ceiling_uvs_follow_the_room_tile_frame`,
  `test_ceiling_grid_decals_snap_in_the_rooms_own_frame` and the checker's
  fixture test.

### 11.5 Pool tile sheen (delivered)

`assets/catalog.json` raises the three pool tile materials through the existing
response system — `specular 0.22/0.22/0.20 → 0.3` and
`shine 0.3/0.3/0.28 → 0.4` for `core:pool_tile_deck_01`,
`core:pool_tile_basin_01` and `core:pool_tile_wall_01` — the visible end of the
guide's restrained glazed-tile band. The texture art, tiling and absence of a
reflection mode are unchanged; a regression test
(`materials::tests::pool_tiles_keep_a_restrained_visible_sheen`) pins the
resolved values, roughness `0.6` and the tiling. Before/after captures:
`docs/screenshots/run06-pool-sheen-before.png` / `run06-pool-sheen-after.png`
(quality High; the local `settings.json` runs Low, where the whole surface
response is gated off, so a Low capture would show no difference by design).

### 11.6 The map geometry checker (delivered)

`src/geometry_check.rs` plus a `--check-geometry` mode in `main` that runs
before any SDL/wgpu bootstrap. It parses the level, runs
`loader::validate_level`, then the same preparation pass the game uses
(`prepare_level`), builds the static mesh with
`render::build_level_geometry_with_catalog_and_materials` and derives collision
from `LevelDef::collision_aabbs`, then reports findings.

CLI: `places --check-geometry --level <path-or-id> [--json <path>]
[--markers <path>] [--markers-obj <path>] [--strict] [--quiet]`. Exit `0` no
confirmed defects, `1` confirmed defects (or any warning with `--strict`), `2`
usage/IO/parse failure. Human output prints every finding with its check id,
element id, message and world position; `--json` writes the versioned
`places-geometry-check` report; `--markers-obj` writes a cross per finding for
a viewer.

Checks (confirmed errors first): `level-invalid`, `degenerate-face`,
`non-finite-vertex`, `duplicate-surface`, `collision-duplicate`,
`collision-mismatch` (an authored solid missing from the engine's collision
set), `opening-overlap`, `curved-invalid`, `curved-collision-gap`, and
`curved-collision-overshoot` (non-coarse curves only: a coarse tessellation
gets the actionable `curve-coarse` warning instead, because its AABB slack
follows from the tessellation); heuristics: `coplanar-sliver` (sub-10 cm² joint
slivers), `reversed-face` (two coplanar faces with opposite normals — a
genuine reversal or a legitimate back-to-back pair such as two stacked walls'
caps, which the checker cannot tell apart), `overlap-emission` (two rooms'
floors/ceilings sharing a plane, documented behaviour), `ghost-collider` (a
surface-tight rectangular collider with no mesh on any face; guardrails and
curved primitives are exempt), `opening-unused`, `curve-coarse`,
`missing-wall`, `room-leak` (the walkable space reaching the *void*, not
another room), and `spawn-outside-room`. Heuristic warnings are suppressed only by narrow
`geometry_intent[]` rectangles with a `check` id and a note; errors are never
suppressed. Fixtures: `tests/fixtures/levels/geometry_broken.json` (planted
defects) and `geometry_intentional.json` (valid curves, openings, a rotated
ceiling grid and an annotated open bay), plus
`tests/fixtures/levels/invalid/geometry_invalid.json` (loader-rejected
degenerate curves; living in a subdirectory keeps the Python level scanner
from failing on it).

Findings and repairs on the shipped levels:

* **Places Demo** — run exit 0: 0 errors; 1 `coplanar-sliver` warning at the
  baseboard corner joint in the office (a 0.00031 m² hidden overlap,
  deliberately reported rather than hidden); 2 `missing-wall` warnings
  suppressed by one `geometry_intent` annotation (the corridor loop's return
  leg opens into the stair hall — the intended route). Investigating the
  checker's first run also drove the baseboard end-face repair (§11.3.4),
  which removed 13 real coplanar overlaps from the prepared demo.
* **The Pit** (`levels/level0_pit.json`, a user drop-in, preserved) — run exit
  1: 16 `duplicate-surface` errors (4.3 cm² each) where two perpendicular
  equal-height wall caps overlap at a corner; the engine's wall-cap emitter
  does not clip top caps against abutting walls that were not coalesced into
  one unit. 98 `coplanar-sliver` warnings of the same class, 52 `missing-wall`
  warnings around the intentional carpet holes/shafts and the stacked storeys.
  The layout was not modified. The deferred fix is recorded in §11.10.

### 11.7 Offline compute and multicore work

No new expensive Python solver was created. The two Python tools this run
re-ran are trivial and were measured on the final tree:
`python3 tools/assets/validate.py` 0.07 s, `python3 tools/textures/build.py
--check` 0.05 s. The checker itself is a single-pass Rust CLI over the level:
measured on the release binary, **0.12 s** for Places Demo and **0.29 s** for
The Pit (the largest level in the checkout). Process- or thread-based
parallelism would cost more than the work it partitions, so the tool is
deliberately serial and the serial run is its reference; the requirement's
"improve the algorithm before scaling up" is met by the checker's bounded
spatial hashing (1 m triangle hash, quantised plane buckets, capped finding
lists) rather than by brute force. The prop/entity toolkits' existing
process-based worker budget is unchanged and was not on this run's critical
path.

### 11.8 Remaining issues / next-run dependencies

* **Wall top caps at non-coalesced perpendicular corners (The Pit's 16
  errors).** `emit_wall_caps` subtracts the unit's own steps and floor
  coverage but not another wall's solid where two walls' tops share a plane
  and the pair was not coalesced. A minimal repro ships in
  `tests/fixtures/levels/geometry_broken.json` (the two `core:wallpaper_stained_01`
  walls at `x=0, z=4`: 3×0.3 and 0.3×3 m, both 1.2 m tall, meeting at one
  corner — 47 `duplicate-surface` pairs in that fixture, most of them from the
  duplicated guardrails and this corner). A correct fix needs deterministic
  ownership (the later-emitted wall gives up the overlap) and must keep the
  `surface_audit` cases green; deferred rather than rushed.
* **Demo baseboard corner sliver.** The remaining Places Demo warning is a
  0.00031 m² hidden cap overlap between two perpendicular automatic baseboard
  runs at an inside corner. It is sub-visible and reported on purpose; the
  fix belongs to the same corner-ownership work as the wall caps.
* **The Pit warnings are dominated by intentional features.** 98 slivers plus
  52 missing-wall warnings; the level is a user drop-in with 15 carpet holes
  and deliberate stacked open storeys. Annotating it is the owner's call; the
  reports are in `target/agent-work/run06-reports/`.
* **Engine diagnostic fixtures report pre-existing overlaps.** The checker
  flags 14 `duplicate-surface` cap pairs in `rendering_diagnostic` (automatic
  baseboards running over one another on a shared cap plane) and 16
  `reversed-face` warnings in `vertical_diagnostic` (coincident hidden caps of
  two stacked walls — a legitimate construction). Both are engine test
  scaffolds built to stress lighting, with pinned expectations; they were left
  untouched, and `rendering_diagnostic` is the second repro for the corner/trim
  ownership work above. The `reversed-face` class was demoted to a warning
  precisely because this fixture proved the checker cannot separate a hidden
  back-to-back pair from a reversal without solid semantics.
* **Reviewer-found false positive: a legal 5-segment pillar** previously exited
  1 with `curved-collision-overshoot`; the check now skips a coarse
  tessellation, and the same level exits 0 with only `curve-coarse`. The
  `geometry_broken` fixture's 4-segment pillar no longer clears the threshold
  "by 1.6 mm" — it is exempt by the same rule.
* **Usage errors now exit 2** (`--bogus`, `--level` without a value); the first
  cut returned Rust's `Err` exit 1 before reaching the checker's own status
  path (`geometry_check::exit_usage`).
* **No decals on curved surfaces** (documented limitation). A curved-surface
  decal would need a conforming decal mesh; not in scope.
* **Planar reflections on curves** are skipped by the existing
  non-planar-material guard (a curved face is reported and keeps its sheen);
  probe reflections are the right choice there, as documented.
* **The checker sees the asset-less mesh** (prop placeholders, no GLB
  interiors) and never the rendered frame; a clean run proves no *measured*
  geometry defect in that interpretation, not watertightness.
* **`settings.json` runs Low locally**, where the surface response (and
  therefore the pool sheen) is gated off. Any visual re-check of the sheen
  must run Medium/High or `PLACES_QUALITY=high`.

### 11.9 Checks and results

| Check | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | exit 0 |
| Lint (strict, including the gate's `clippy::cargo` set) | `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` | exit 0, no warnings |
| Tests (final tree) | `cargo test --workspace --no-fail-fast` | **1265 passed, 0 failed, 8 ignored** (665.3 s, exit 0), including the run-06 regressions (`geometry_check` fixture suite, arc UVs and end caps for both sweep signs, guardrail post rule, the demo handrails on the nosing line, the ceiling tile frame and decal snapping, pool sheen) |
| Assets + levels | `python3 tools/assets/validate.py` | OK (137 assets / 48 placeable, 13 levels, 0 warnings) |
| Textures | `python3 tools/textures/build.py --check` | OK (47 textures, 40 pre-existing preferred-size warnings) |
| Props | `python3 tools/props/build.py --check` | OK (48 props, budgets within limits) |
| Package gate | `python3 -m unittest tests.test_package` | 42 tests, 1 pre-existing failure (`icon.png` 1254 px against the 512 px test limit, untouched) |
| Compiled build (windowed) | `python3 -m unittest tests.test_compiled_build` | OK (8 tests) |
| wgpu bootstrap (windowed) | `python3 -m unittest tests.test_wgpu_bootstrap` | OK (14 tests; one stale expectation updated: the boot demo now spawns the drum *and* the run-05 floating duck, 2 objects / 772 vertices) |
| Checker, Places Demo | `./target/release/places --check-geometry --level places_demo` | exit 0: 0 errors, 1 warning (the hidden 0.00031 m² baseboard-corner sliver), 2 `missing-wall` warnings suppressed by one `geometry_intent` annotation |
| Checker, The Pit | `./target/release/places --check-geometry --level levels/level0_pit.json --json target/agent-work/run06-reports/pit.json` | exit 1: 16 `duplicate-surface` errors (non-coalesced wall top caps, §11.8), 98 `coplanar-sliver` warnings, 52 `missing-wall` warnings around intentional holes/open storeys; layout untouched |
| Checker fixtures | `cargo test --bin places geometry_check::tests` | 7 tests pass: the broken fixture reports every planted defect, the intentional fixture has 0 errors and only sub-visible slivers, the shipped demo has 0 errors, the invalid fixture names each degenerate curve, curves have tight collision and material variety, walking/crouching/landing behave, and the CLI/report shape is stable |
| Arc/pillar geometry | `cargo test --bin places test_arc_wall_uvs`, `test_arc_end_caps`, `test_round` | pass: arc-length tiling on each face, outward end caps for both sweep signs, degenerate diagnostics, defaults |
| Guardrail post rule | `cargo test --bin places guardrail_posts_are_even_and_never_crowded` | pass: even runs keep both end posts, the 2.5 m/1.2 m remainder moves the last post to the end, a 0.55 m stub keeps both |
| Ceiling frame and vent | `cargo test --bin places test_ceiling` | 5 tests pass: frame round-trip, ceiling UVs, decal snapping in a rotated/offset frame, idempotence |
| Demo handrails | `architecture_audit::test_the_demo_stair_handrails_follow_the_nosing_line` | pass: both rails track the 0.84 nosing line and end at the last nosing |
| Pool sheen | `cargo test --bin places pool_tiles_keep_a_restrained_visible_sheen` | pass: specular 0.3, roughness 0.6, tiling and no reflection mode |
| Captures inspected | `PLACES_CAPTURE` at `PLACES_QUALITY=high` | `docs/screenshots/run06-stairs-rails-before/after/diff.png`, `run06-pool-sheen-before/after.png`, `run06-ceiling-vent.png`, `run06-arc-wall.png`, `run06-round-pillar.png` |
| Offline compute measured | see §11.7 | checker 0.12 s (Demo) / 0.29 s (The Pit) release; `validate.py` 0.07 s; `build.py --check` 0.05 s — serial is the fastest safe configuration |
| Working tree | `git status --short` | only the intended run-06 additions; no commits, worktrees, resets or stash |

### 11.10 Independent review

<!-- REVIEWER_06 -->

### 11.10 Independent review

The independent validator / false-positive reviewer worked on a pinned copy
(`target/reviewer-06/repo`) and re-derived the claims with its own probes: 70
explicit CONFIRMED/REFUTED/UNVERIFIABLE findings, all with commands and
numbers, in `target/agent-work/reviewer-06.md`. It independently confirmed:
positive-sweep winding/caps/end caps, side UVs (worst error 0.00000 m against
the 0.02 m budget), cap plan mapping, full-ring closure, pillar collision
coverage and tightness (no whole-circle square), the walking/crouching/jumping
probes, the checker's 0/1/2 statuses, `--json` schema, marker JSON and OBJ
validity, byte-identical repeated runs, intent suppression, `--strict`, the
demo rail pitch/posts/UVs/normals with zero guardrail overlaps, the vent PNG
contract and snapping on independently computed frames, the pool sheen values
and no weakened seam tests, Places Demo exit 0 and The Pit's exact counts.

It refuted or flagged four claims and three defects, all resolved in this
run's final tree:

1. **Negative-sweep arc end caps faced inward** (real bug, not caught by the
   checker because it reads collision boxes, not winding). Fixed in
   `emit_arc_wall` by carrying the sweep's sign into the end tangents, pinned
   by `architecture_audit::test_arc_end_caps_face_outward_for_both_sweep_signs`
   (verified to fail against the old code).
2. **Usage errors exited 1, not the documented 2** — fixed with
   `geometry_check::exit_usage`; `--bogus` and a valueless `--level` now exit 2.
3. **The handoff claimed a demo-railing regression test that did not exist** —
   added (`architecture_audit::test_the_demo_stair_handrails_follow_the_nosing_line`),
   plus a unit test of the emitter's post-position rule
   (`render::common::architecture::tests::guardrail_posts_are_even_and_never_crowded`).
4. **A legal 5-segment pillar was a `curved-collision-overshoot` false
   positive** — coarse tessellations are now exempt from that error and get the
   actionable `curve-coarse` warning instead; the reviewer's `pillar5.json`
   probe now exits 0.
5. **16 `reversed-face` errors on `vertical_diagnostic` were hidden
   back-to-back caps of two stacked walls** (proved at y = 2.0,
   x 9.85..10.0, z 0..4) — the check is now a heuristic warning with the
   ambiguity documented, because the checker has no solid semantics to tell a
   reversal from a legitimate stacked junction.
6. **The stored demo evidence report was stale** (two pre-fix
   `ghost-collider` entries) — the reports under
   `target/agent-work/run06-reports/` were regenerated from the frozen binary.
7. **A tautological assertion and minor doc drift** (ghost-collider
   exemptions, the decal panel module wording, the fixture vent count) — fixed.

The reviewer also recorded two honest limits: pillar collision's row-AABB slack
is +2.4%…+15.5% of the radius across legal tessellations (not the 5% an early
internal test assumed), and arc collision leaves a chordal lens of at most
2.61 mm between the drawn chord and the sub-arc; both are sub-visible and
introduce no pass-through. It confirmed the main checkout was never edited,
committed, reset, cleaned or worktreed. Its copy predates the final baseboard
sliver-threshold change and the fixes above, so its full-suite number is from
that copy; the final tree's suite is in §11.9.
