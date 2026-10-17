# Stage 5: gameplay and environment authoring

Implemented on `0.12.0-staging`, starting at `69363d0`. Stage 4 CI
[39074519428](https://github.com/csd113/Places/actions/runs/39074519428) completed
successfully. No shipped map, model, texture, sound or catalog asset changed.
Stages 6–7 remain the next sequential work. Model integration, the rejected newer
Spoonerman, menus/distribution and the future Closing Time mode are outside this
change.

## Authoring workflow

The existing central viewport and compact scene/transform panel remain. One
grouped creation sheet adds built-in lights, ambient/positional sound emitters,
entities, doors, triggers, box fog, ground fog and spawn points. Selection-specific
configuration sheets expose capabilities without adding permanent panels.
Normal 1440×816 and minimum 960×544 native layouts were visually reviewed.
The trigger action list and light/sound/door/spawn sheets were inspected as part
of real native editing, rather than only after implementation.

Lights configure power, range, colour, face emission, switching and mounting.
Sound emitters configure installed cues, gain, ambient/positional policy,
looping and play-on-load. Props toggle physics per placement, with pickup, mass,
solidity and activation groups. Entity/spawn templates offer existing shared
idle/wander/follow/flee behaviours. Doors default closed; two-way opening chooses
the side away from the occupant and retains it during interrupted motion.

Triggers resize through the ordinary position/dimension inspector and display
translucent authoring-only bounds. Filters select the player, general or held/
unheld physical items, exact items and exact entity identities. Ordered actions
play a configured sound, open a door, spawn at a movable point or activate a
physics group. Spawn points select reusable built-in templates and bounded
lifetimes; copies receive independent one-active-spawn groups. The engine owns
all behaviour. Existing additional bindings and unsupported raw fields survive
ordinary editing.

Native testing caught and corrected configure-before-actions failure: an empty
trigger now stores validated `editor_trigger_policy` metadata until its first
action materializes a valid runtime binding. Removing the last action restores
the draft. Runtime empty-action validation remains strict. New decorative
entities are passive by default so physics can be enabled without an unrelated
interaction component preventing the edit.

## Placement and protected intent

Props support yaw, pitch and roll through the shared placement matrix, renderer,
prepared geometry, picking, conservative collision/navigation envelopes,
attached point offsets and full rigid-body orientation. Physics remains frozen
on load until interaction or activation. Negative floor-relative Y and solid
intersections remain authorable; no transform-time correction pulls scenery out.
Geometry revision **3** requires recompiling prior prepared output.

Explicit mounting measures actual model base/top/back bounds, aligns floor props
to continuous floor/ramp normals and wall fixtures to the outward wall normal,
and uses a 1 mm clearance. Ceiling mounting accounts for the housing back rather
than the luminous aperture. Snap is optional; manual adjustment and undo remain.
Native mounted values in the 3.2 m acceptance room were 3.199 m for the Office
panel, 3.196 m for the Pool disc (whose local top is +0.003 m), and X=0.251 m for
the wall model against the X=0.25 m interior face, yaw 90°.

`Ignore for this placement` binds one warning to stable raw instance identities,
the measured issue and affected source context. Save/reopen, review/clear and
undo work; meaningful transform/support/geometry changes reactivate the warning.
Other findings on the same placement remain independent. Duplicate/malformed
identities and malformed metadata are rejected, and runtime/source semantic
validation stays active.

Individual preview/apply and the shared batch correction gateway skip ignored
issues and protect their placement/support context. An unrelated correction
that would affect protected context is refused for explicit review. The native
case kept the ignored rotated desks byte-for-byte while removing an unrelated
duplicate wall; clear/undo restored the same active decision. Focused batch
regressions prove protected preservation and clear-ignore eligibility. There is
no new bulk-fix UI.

The fog round-trip regression also exposed the default JSON parser rounding the
last digit of the editor's own saved floats. Enabling `serde_json`'s precise
`float_roundtrip` parser fixes rereading generated coordinates/colours/bounds;
duplicate-key and unsupported-precision safeguards remain enabled.

## Shared runtime and bounds

`engine::entities` retains occupancy, events, actions, audio and spawn lifecycle.
`sound_emitters[]` supplies sound-only entities with absolute world XYZ and a
per-emitter spatial override. `TriggerFilterDef::Entity` watches a placed identity
or explicit authored spawn name; generation-safe membership handles replacement.
Physical actors retain the swept solver path/AABB; other actors use live base
points. `dispatch_actions_from` separates the action owner from occupant origin
for reusable door/action dispatch. Door action counts now propagate through
queued events and sequences as well as the actual state change.

A load-time balanced AABB tree queries swept actors. Previously occupied volumes
remain candidates for exits, disappearance and queue retries. Item bitmasks avoid
all-item scans per volume; exact identities resolve once per watched identity.
Bindings are indexed by owner and only active cooldowns tick. Source caps remain
4000 volumes, 128 physical items and 16 bindings per owner. Existing event/chain
budgets remain. `trigger_query_stats()` exposes exact diagnostic counts.

Regional and separate ground fog share the existing bounded 16-volume uniform
array and quality prefixes 2/8/16. Eye-to-surface rays clip against all six box
faces and integrate four fixed profile samples; walls beyond a volume receive
its haze and lower storeys do not inherit upper-floor fog. Ground layers fade
upward independently. The strongest regional optical depth wins, plus global
atmosphere. No extra rendering pass or per-frame CPU map scan was introduced.
The editor toggles neutral inspection versus authored vertex lighting/atmosphere;
Play uses the compiled Full lighting and complete ceilings.

## Validation and native evidence

All final gates passed:

- `cargo fmt --all --check` and `git diff --check`.
- Strict workspace/all-target/all-feature Clippy.
- Locked release workspace/all-feature build of production executables.
- `cargo test --workspace`: **109 passed**, zero failures; fixture bodies total
  0.20 s, with no map bake, GPU or full shipped-map dependency.
- Python source suite passed: **368 tests**, three optional skips.
- Asset catalog: 593 assets, 370 placeable, eight themes, zero warnings.
  Read-only texture and prop checks passed; texture checking retains 96 existing
  preferred-size/manifest warnings.
- Full combined WGSL parsed and passed Naga semantic validation, followed by
  actual Metal rendering in the native acceptance.

Native evidence and the copied direct executables are under
`local/verification/0.12.0-task5/`, outside `target/`. They require this checkout's
installed assets and host SDL3; they are not clean-install distribution signoff.
No `.app` was launched.

- `native/editor-acceptance/`: successful 196-step native editor sequence,
  creation/configuration, physics on/off, pending trigger policy and three action
  kinds, resizing, both fog controls, preview, full rotation undo/redo, exact
  mounts, save and framebuffer captures.
- `native/editor-reopen-play/`: exact saved reopen, ordinary Save+Build+Play,
  Full/Static package, direct import/player execution and unchanged return.
  Package validation passed and `verify --require-current` reported identical
  fingerprints. The Full player retained the desk at yaw 15°, pitch 20°, roll
  90° and Y≈−0.2 m without physics or placement correction.
- `native/ignore-acceptance/`: persisted specific ignore, unrelated same-item
  warning still visible, clear/undo, preview/apply of another wall correction and
  saved exact protected desk records.
- `native/gameplay-final/`: opposite door swings, player-trigger sound/door/spawn/
  physics activation, exact entity/item-filtered sound actions, frozen-to-active
  item transition, ambient/positional voices and captured nonzero mixer PCM.
- `native/bathroom-final/`: the existing Arcade restroom entry leaf and local
  source geometry, closed→+90°→closed→−90°→closed. The shipped Arcade source was
  not redesigned or modified.
- `native/ground-fog/`: matched on/off low-eye captures show a distinct ground
  layer; 497,851 pixels changed by more than five channel values. Box fog beyond
  its volume is visible in the gameplay captures.
- `native/assertions.json`: **41/41 passed**. Delivery PNGs and launch script are
  listed in `delivery-manifest.json` and `try-editor.sh`.

After confirming no active Cargo/Rust/player processes, one successful
`cargo clean` removed 12,406 files / 11.8 GiB. The preserved editor then reopened
the exact saved source with `target/` absent, a clean document and unchanged
payload hashes (`native/post-clean-assertions.json`). The host's sccache wrapper
needed the same `RUSTC_WRAPPER=` override used for build gates; the initial
cleanup attempt stopped before removing anything.

Earlier diagnostic runs are retained with their logs, including the workflow
bugs above and two corrected scene-list/modal assumptions in the acceptance
script. The named final evidence is the passing rerun.

## Performance evidence and limits

Matched sequential native direct-binary runs used Apple M2 Pro / Metal,
**observed Immediate**, physical **1920×1280**, V-sync off and no frame limiter.
Each discarded 600 warmup frames and measured 6000 frames. No compilation or
other game window ran concurrently. Cadence exceeded display refresh; presentation
averaged 0.019–0.037 ms, with the stress cost in render/GPU backpressure. These are
single-run observations, not universal performance claims. Only average FPS is
reported:

| Case | Stage 4 | Stage 5 |
| --- | ---: | ---: |
| Six-metre room, 16 triggers, one fog region | 809.93 | 661.84 |
| Same room, 16 fully overlapping fog regions | 557.31 | 102.28 |

The correct sightline-integrated fog adds measurable GPU cost versus the old
surface-only approximation: −18.3% in the representative case and −81.6% in the
deliberate maximum-overlap stress. This is a known cost, not a claimed speedup.
Avoid unnecessary overlapping fog; quality prefixes bound work. Worst-case fog
is 16×4 profile samples per fragment and remains expensive in heavier scenes.

The explicit no-bake CPU tool measured 4000 trigger boxes: empty sparse query
0.665 µs average (zero sampled volumes), 128 sparse items 19.055 µs (128 volumes/
128 sweeps), and the deliberately pathological all-overlap case 38.065 ms
(4000 volumes/512,000 sweeps). Relevant active sets still cost work; the source
ceiling is not a promise of 60 FPS for every all-overlap construction. Raw
results, source and counters are retained beside `performance/summary.json`.

Mounting refuses gabled/open fixture ceilings, incompatible fixture families and
footprints crossing discontinuous support, without moving the object. Fixtures
retain yaw-only mounting; fitted spans/floats retain their own upright poses.
Tilted finite-area prop lights retain horizontal footprints. Full transport uses
actual prop triangles; Static/legacy player envelopes remain conservative.
Unknown-size legacy props and unsupported geometry can invalidate ignores more
broadly than necessary. Existing bounded safety-review limitations still apply.
Native Windows/Linux and distribution acceptance are not claimed.

## Stage 6 handoff

Continue with Luau, then stage 7 export/import. Reuse `EntityWorld`,
`ActionDef`, `dispatch_actions_from`, exact identity filters, generation-safe
audio/spawn lifecycle and the bounded trigger/event machinery. Keep runtime
logic outside editor UI. Preserve raw document history, stable placement intent,
atomic save and guarded correction gateways. Script execution must not bypass
source/asset validation or silently alter protected placement intent.

The final commit, payload hashes, launch paths, CI identity and cleanup evidence
are recorded in `local/verification/0.12.0-task5/stage5-handoff.json`. Model
integration remains deferred until all stages 4–7 finish. The later Closing Time
mode is context for reusable authoring only; it has not been built here.
