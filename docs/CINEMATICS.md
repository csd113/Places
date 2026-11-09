# Live in-engine cinematics

A cinematic is an opt-in **ordinary sequence** with an out-of-body camera. Build
its rooms and props normally, then start the sequence through an ordinary
trigger/event binding. NPCs use the existing bounded Luau/AI actions; completion
uses the existing keyed `sequence_complete` event. There is no video playback,
second scripting runtime or timeline editor.

## Native authoring

1. Create a normal player Trigger around the activation area, or select a prop
   with an enabled **Components > Interaction** (including native Feltlings).
2. Open **Advanced / All tools > Cinematics**. Choose **New**, or select a saved
   cinematic. The selected eligible object becomes **Controller**; the controller
   button cycles player triggers and interactable props. Props start on explicit
   player **interact**, triggers on **enter_volume**.
3. Fly/orbit the editor view and choose **Capture start**. This does not change
   the player spawn or move level geometry.
4. Add and order **Move**, **Follow**, **Orbit**, **Wait** or **Script** rows.
   Select a row to edit its numeric fields and aim. Position/offset values are
   world metres; yaw zero faces -Z, yaw +90 faces +X, positive pitch looks up.
   The scalar fields are paged. Capture an endpoint from the editor view when
   authoring a Move. Close, move the editor view and reopen Cinematics to keep
   the pending draft while capturing another view.
5. Choose **End: Return control**, or a destination from the owning project.
   **Preview** validates the complete transaction without changing source/history.
   **Cancel** discards the draft; **Close** retains it for camera capture.
   Apply validates the sequence, its exact helper bindings and the declared
   transition together as one undoable edit. Save persists the source normally.
6. **Test in editor** exercises the camera, triggers and NPC scripts without a
   lighting bake. Stop discards temporary play and restores the editor
   view. Escape closes an active cinematic while retaining Test; a later Escape
   returns to editing. Embedded Test reports a reached cross-level transition rather than
   loading another level. **Build+Play / complete project export** validates
   actual destination loading.

The helper owns only its named start/completion bindings. Unrelated components,
script source, event actions and unknown source fields survive. A stale draft or
invalid field cannot partially change wiring. Existing ids/controllers are
stable; remove ordinary references before removing a referenced cinematic.
Manually modified helper actions are refused, not overwritten.

For triggers, **Once per reset** retains the ordinary binding policy: entry
consumes it even if a busy start is refused or its run is canceled. **Repeat**
allows another entry after leaving.

For interactable props, **Once on success** uses an ordinary helper Bool state
`cinematic_<sequence-id>_done`, initially false. The interact binding is repeatable
and checks that state; exact-key successful completion sets it true. Busy starts
and Escape close consume no success, so the player can interact again. **Repeat**
omits the state condition. The camera lease still refuses active duplicate starts;
no queued/restarted run or false completion is introduced. Normal reset rearms
the state. Add independent `sequence_complete` bindings in Events for quest
progression; the sheet refuses manually changed helper actions/conditions.

## Camera rows

- **Move** travels from the previous camera pose to a world position, with
  linear or smoothstep easing. Aim can be held, interpolated to explicit angles,
  or kept on a point/live entity.
- **Follow** uses the target's live position plus a fixed **world-axis** offset.
  It does not rotate that offset with the actor. Choose Look at for the aim.
- **Orbit** travels around a point/live entity with a radius, height, starting
  angle and signed sweep. Angle zero is the target's -Z side. Look at normally
  aims toward the orbit center.
- Follow and Orbit establish explicit shot-start poses. They are cuts unless
  the preceding endpoint already matches. No automatic blend or spline can
  overshoot an authored narrow passage.
- **Wait** retains the current pose. **Script** invokes an existing exported
  project-local handler through the same sandbox as any other script action.

Camera paths are intentionally free, not physics bodies. They do not sweep
around walls automatically. Keep them clear of geometry and inspect the native
view. Existing user FOV is retained; roll, FOV keyframes and a Skip button are
not part of this first version. Enter/PgDown advances dialogue. Escape closes
an active cinematic, clears its speech and restores player control without a
successful completion or destination transition. Cinematics have no Back/reread
or skip-message action and display no subtitle control hints. Outside a
cinematic, Escape retains normal Pause/return behavior.

## Source contract

The level remains `schema: "places-level", format_version: 1`. Omitting
`cinematic` preserves existing ordinary sequence behavior. A minimal wiring is:

```json
{
  "volumes": [{
    "id": "view_trigger", "x": 1, "z": 1,
    "width": 2, "depth": 2, "height": 2,
    "bindings": [
      {"on": "enter_volume", "once": true,
       "actions": [{"action": "start_sequence", "sequence": "room_view"}]},
      {"on": "sequence_complete", "key": "room_view",
       "actions": [{"action": "change_level", "level": "next_room"}]}
    ]
  }],
  "transitions": ["next_room"],
  "sequences": [{
    "id": "room_view",
    "cinematic": {"start": {"position": [2, 2, 4],
                            "yaw_degrees": 0, "pitch_degrees": 0}},
    "steps": [
      {"step": "camera", "seconds": 3,
       "motion": {"kind": "move", "position": [2, 4, -4],
                  "easing": "smooth"},
       "aim": {"kind": "look_at",
               "target": {"kind": "point", "position": [2, 1, -6]}}},
      {"step": "wait", "seconds": 1}
    ]
  }]
}
```

This is a fragment, not a complete level: supply ordinary rooms/spawn/assets and
an owning project declaring `next_room`. Completion bindings belong to the
entity **running** the sequence, which defaults to the triggering owner. An
explicit `start_sequence.target` changes that owner, so put completion wiring
there instead. Return-control needs no completion action/binding.

Camera targets are `{"kind":"point","position":[x,y,z]}` or
`{"kind":"entity","id":"npc_id","offset":[x,y,z]}`. Entity ids name authored
instances, not display labels or future script-spawned names. Aim is
`{"kind":"keep"}`, `{"kind":"angles","yaw_degrees":0,"pitch_degrees":-15}`
or `{"kind":"look_at","target":...}`. Camera motion is tagged `kind`:

```json
{"kind":"follow", "target":{"kind":"entity","id":"npc"},
 "offset":[0,2,3]}
```

```json
{"kind":"orbit", "target":{"kind":"point","position":[4,1,4]},
 "radius":3, "height":1.5, "start_degrees":0, "sweep_degrees":180}
```

A script row is the existing action step:

```json
{"step":"action", "action":{"action":"run_script",
 "script":"scripts/scene.luau", "handler":"begin"}}
```

Handlers can use `api.navigate`, keyed `api.ai_task`, normal animations, doors,
state and sound. Calls are bounded and atomic under the existing Luau policy,
not awaited coroutines. Finishing camera travel does not prove an asynchronous
NPC navigation task arrived. Use existing task/state/animation events when
arrival must control progression. Canceling a camera does not undo prior
script effects or stop unrelated NPC tasks/VMs.

Bounds: each camera row takes 0.05..120 simulation seconds; existing sequences
have at most 128 rows and levels at most 1024 sequences. Cinematics cannot loop,
need at least one camera row, and have at most 600 declared seconds plus a
600-simulation-second watchdog. Position/offset components are finite within
±100000 m; orbit radius is 0.05..1000 m, sweep ±720 degrees, authored yaw/start
angle ±360000 degrees and pitch ±89 degrees. Bad references/numbers are refused.

## Ownership and interruption

Only one cinematic holds the local camera. Repeated/competing starts are refused
without restarting effects. Ordinary sequences on other entities continue.
`stop_sequence` cancels without a success event; a sequence `stop` row means a
successful end. Owner/active target removal, reset, recovery, unload or invalid
ownership cancels safely. Recreated text ids cannot inherit old target handles.

During the lease, player movement/look/interact/pickup/throw/flashlight input is
suppressed, but gravity, collision, NPCs, scripts and mechanisms remain live.
The physical player, held items, flashlight and AI observations are not moved to
the camera. No invulnerability or old-body-position rollback is introduced.
Pause/settings/focus loss freeze simulation; graphics-only rebuilds preserve the
same world and camera. On release, stale controls/mouse edges are discarded and
fresh input works normally.

One published effective eye drives rendered frustum/occlusion, view-dependent
lighting/reflections and cinematic spatial/environment audio. Ordinary camera
modes retain physical-player ears. A late event accepting a cinematic takes
input immediately; its view/listener publish together on the following frame.
Camera movement does not rebake the level or turn the camera into an AI target.

A successful run emits one `sequence_complete` with its exact id key. A full
bounded event queue retains the finished run and retries without replaying
steps, while player control is already released. New cinematic admission waits
until that terminal event is accepted. Cancellation emits no success. Active or
pending-finish sequences retain the existing checkpoint refusal; save after the
sequence completes, not halfway through it.

Compiled cinematic maps require `live-cinematics-v1`; older players reject the
capability instead of silently running only the completion actions. Complete
project transitions retain the normal `project-transitions-v1` package rules.
