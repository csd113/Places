# Flush hollow-metal exit doors

Single and paired **flush hollow-metal exit doors with surface-mounted
vertical-rod exit devices (panic hardware)** are original Blender/GIMP assets.
These are generic visual/gameplay models, not a brand, fire-rated assembly,
certified egress product or real-world safety specification.

## Place and use

Choose **Place hollow-metal exit door** or **Place hollow-metal exit door pair**
in the editor's Doors browser. These choices place a complete new compound prop
at the view target, rather than reskinning an unrelated existing slab door.
They also appear under Models as `core:hollow_metal_exit_door_single` and
`core:hollow_metal_exit_door_double`. Place chosen prop installs the same exact
mechanism preset. Select its scene/viewport entry and use ordinary prop position,
yaw and uniform scale controls. Test, Stop, Undo/Redo and save/reopen retain its
explicit node boxes, components and bindings. Replacing an active mechanism's
model is refused atomically, so custom wiring cannot disappear silently.

Fit an actual wall opening, not a door-shaped texture on a solid wall. At scale 1:

| | Single | Pair |
| --- | ---: | ---: |
| Nominal wall opening | 1.0 × 2.15 m | 1.9 × 2.15 m |
| Closed occupied bounds | 1.152 × 2.21 × .252 m | 2.052 × 2.21 × .252 m |
| Frame clear width | .984 m | 1.884 m |
| Conservative fully open body-path width | .8605 m | 1.6370 m |
| Triangles / joints | 728 / 0 | 1,268 / 0 |
| Posed collision boxes / grounding subjects | 9 / 3 | 15 / 4 |

Metres, +Y up, +Z push side, leaves open toward -Z. The single is left-hinged;
the pair opens both mirrored leaves together. Host wall thickness is 160 mm.
The local frame/wall plane is **Z=-.028 m**. At yaw zero, put the prop origin
.028 m beyond the desired wall centre plane. For another yaw, rotate the local
`[0,0,.028]` offset with that same yaw. Keep real leaf/frame origins and reserve
the complete sweep in `tools/doors/metadata/asset_gate_report.json`, plus clearance.
Do not nonuniformly stretch the frame to fit another host depth. Floor placement
is the room floor plus prop `y`; the model's bottom is zero. Clear height above
the 8 mm visible threshold is 2.134 m. These are practical game dimensions.

E toggles the unlocked mechanism. It starts closed and unlocked, opens over the
original 1.1-second `exit_open` clip, and stays open. Another E closes it; an
in-flight toggle reverses from the accepted pose. There is **no autoclose** and
no independent pair-leaf control. A prospective player, NPC, static wall, item or
other door obstruction holds the pose and retries when clear. Closing a locked
open door remains permitted, like the existing slab API. No new sound cue, gain,
master file or audio allocation is introduced.

## Authoring and API

Canonical explicit presets live in `assets/shared/doors/hollow_metal_exit_door_*.json`.
They use the existing `animated_door` rigid mechanism, `solid:false`, `occludes:false`,
an interactable plus paused non-looping animation component, and an ordinary
`interact -> toggle` binding. Runtime does not inject missing presets into arbitrary
props. The editor applies defaults only on explicit placement/assignment.

Compound doors now accept the common `open`, `close`, `toggle`, `lock`, `unlock`,
`locked`, `unlocked`, `door_open`, `door_closed` action/condition surface. Legacy
`play_animation`/`toggle_animation` still use the same accepted clock and respect
the lock. `animated_door.locked` is optional, default false. Reset restores the
authored open/locked state, colliders, pose and interaction bounds.

`api.door_status(id)` retains slab `phase`, `locked`, `obstructed`, `angle`.
For a compound mechanism it returns `phase`, `locked`, `obstructed`, and normalized
`progress` instead of inventing a single angle for several hinged leaves. Both
are immutable snapshots. A Door use-point can request the compound open end.
Live player/physics/route/NPC collision uses the current oriented solids, and
accepted motion invalidates navigation clearance. This does not add compound
portal auto-opening to the navigation bake; author explicit use-points or script
opening when needed. Optional gameplay checkpoints still explicitly refuse a
level containing clip doors, rather than silently dropping their pose. Ordinary
reset and editor Test restart are supported.

Newly exported compound-door packages advertise `animated-door-actions-v1`
alongside `animated-door-v1`. The newer capability covers shared actions/locks
and per-box grounding selection. Legacy packages without new options remain
readable; packages using locked or excluded grounding boxes must declare the new
capability. No global triangle, texture, joint, character or audio cap changes.

## Collision, grounding and appearance

The fixed frame has **two jambs and one head**, never one filled opening AABB.
Moving cores, panic hardware, closer bodies and articulated links follow their
actual named GLB nodes at exactly the accepted clip fraction. Tiny hinge fastenings
are cosmetic over the jamb contact, not separate wall-overlapping moving blockers.
The visible 8 mm threshold is deliberately not a non-step-aware live-leaf collider.
It is within the player's ordinary step tolerance and cannot seal the open passage.

Each box optionally authors `grounding` (default true). The exit presets select
only jambs and actual leaf cores. Overhead/mounted hardware and redundant panic
hardware do not duplicate leaf contacts, and the threshold does not cast a fake
wide footprint under an empty opening. Floor approximation still uses the shared
support/separation/relevance rules and eight-subject limit. It is not a silhouette
shadow. No cavity array, stuck-on body attenuation or expensive character-self-shadow
workaround is applied. Actual rigid poses still receive ordinary lighting and cast
through the independent flashlight path. This reuses rigid node animation, not the native generated-slab DynamicScene
route. Live physical/AI sight/navigation blockers and flashlight casting follow
the accepted pose. These compound leaves do **not** cast new practical room-light
silhouettes onto prepared room surfaces or become DynamicScene ray casters for
other movable receivers. They still receive switched/prepared lighting. Keeping
`occludes:false` avoids baking a stale closed door into the room. A native slab
mesh cannot simply replace this articulated asset without freezing its leaves or
duplicating visible geometry; no new shadow solver/cache or expensive character
self-shadow option was added. This is not a native slab-lighting replacement.
Static/Full and reflection acceptance must be recorded against the final native
binary, not Blender previews.

## Original sources and reproducibility

- Runtime GLBs and original matching 256² opaque PNG: `assets/shared/doors/`.
- Packed `.blend`, five-layer `.xcf`, 1024² paint master and authoring scripts:
  `tools/doors/source/`.
- Exact source/export/material/rig/animation identities and deterministic gates:
  `tools/doors/metadata/`.
- `tools/doors/CORRECTION.md` records the integration-found hinge defect. Rear/pull
  face hinge axes and barrels now sit 30 mm behind the frame plane (local Z=-.058).
  Real closer links were recomputed, not bypassed in collision. The original
  preparation archive and its previews are historical, not the canonical identity.

Run `python3 tools/doors/check.py --check` and the ordinary asset gates. Authoring
uses Blender 4.3.2 and GIMP 3.0.4; never run artwork exporters blindly in the checkout.
For deliberate changes, copy `source/`, `metadata/`, runtime GLBs as `runtime/`, and
the PNG as `textures/exit_door_materials.png` into a fresh private staging root.
Set `EXIT_DOOR_ROOT` to it; use the scripts and commands in the historical
`tools/doors/PREPARATION.md`, then review/re-export before replacing canonical files.
The `.blend` reopen gate proved byte-identical corrected GLB re-export. The GIMP
paint and XCF are unchanged from the inspected preparation. No downloaded models,
third-party textures or image-generation stand-ins were used.

The dated asset/shadow inventory names the new source identities and separately
records native evidence/limitations. No production map was changed or new final
layout chosen. Windows acceptance remains the coordinator's next distinct gate.
