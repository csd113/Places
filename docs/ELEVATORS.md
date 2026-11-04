# Passenger elevators

Elevators are authored in `elevators[]`, with a walkable cabin, two sliding cabin
leaves, locked landing doors at every local stop, hall call buttons, and individual
cabin destination buttons. The engine advances one pose for rendering and collision.
Doors ease apart over 1.4 seconds. Car motion accelerates and decelerates smoothly;
`speed_mps` bounds peak vertical speed. An occupied doorway reopens a closing pair
and cancels that departure. Additional requests during a trip do not queue or reload.

The exterior follows the supplied November 3 hallway photo: satin gray
centre-opening panels, a narrow brushed-metal surround, dark header, amber
indicator and compact hall call plate. The cabin now evokes a 1970s office lift:
vertical walnut panels, dark reveals, softened wood rails on brass brackets,
a muted speckled linoleum floor, cream ceiling and a warm prismatic diffuser.
The four rigid GLB parts were authored in Blender and share a fitted GIMP PNG
atlas; editable Blender/XCF sources remain beside them. No artwork is generated
at runtime. The existing motion and collision poses still own every transform.
Only the declared number of cabin buttons is assembled, and aiming shows the
authored destination name. E or a captured left click activates that control.

The diffuser has visual emission and one warm attached light that follows the
car, including during physical floor travel. It uses the existing eight-source
realtime light budget, after ordinary character glows; an exceptionally full
light set can exhaust that budget. Wood rail collision matches the visible stock.

## Placement

`x,z` is the centre of the door threshold. At yaw zero the hall is toward **-Z**
and the cabin extends toward **+Z**. The cabin interior is 2.4 m wide, 2.4 m deep
and 2.55 m high. Its 1.35 m doorway is 2.2 m high. Reserve a clear side shaft
at least 2.6 m wide and 2.5 m deep; put local halls outside the cabin footprint.
Place matching 1.5 m wide, at least 2.25 m high openings in each landing wall.
A wall at local Z=0..0.2 fits the supplied surround and call plate. The cabin
front return and door pockets occupy that thickness.

Room floors and ceilings cannot be cut into shafts. Keep the shaft beside stacked
rooms, as the lab does. Admission rejects static geometry, floors and ceilings
crossing the car's usable travel space. Build/export checks this; never rely on
an elevator to cut a room floor or a wall automatically. Local stops must be at
least 2.75 m apart. Use the same rotation for the hall openings and shaft.

```json
"transitions": ["annex"],
"elevators": [{
  "id": "lift",
  "x": 0, "z": 0, "rotation_degrees": 0,
  "initial_stop": 0, "speed_mps": 1.5,
  "stops": [
    {"label": "G", "y": 0},
    {"label": "1", "y": 3.5},
    {"label": "Annex", "y": 0, "level": "annex", "arrival_elevator": "lift"}
  ]
}]
```

`y` is absolute world elevation. A stop without `level` is a physical landing;
a stop with `level` is a project destination. `initial_stop` is zero-based and
must name a local landing. Stops are ordered as buttons top to bottom. There
are at most eight stops per lift and eight lifts per level. Elevators and hinged
doors share a reserved 128-object budget, leaving space for items and effects.

## Editor and loading

Open **Space and Environment > Elevator / floor travel**. The form creates and
edits complete elevator records with Preview, Apply, Undo, record navigation and
delete. Position, orientation, initial stop, speed and the complete stop list are
editable. Existing records are selected with the form's `<` and `>` controls.
The scene preview shows the authored closed car. Embedded Test supports physical
floor travel and positional cues. Use Build+Play / Export project for cross-level
travel; embedded Test retains its existing cross-level limitation.

Declare each external destination in Project's allowed transitions. Export the
**complete project**, so every destination is installed in the same package.
An elevator button can trigger loading only from inside the cabin and only after
the doors close. `arrival_elevator` defaults to the departing lift's ID. A fresh
private destination world is checked for that cabin and a clear arrival pose
before it replaces the current world. The player arrives inside its initial
landing, facing the doors, and the doors open. Old ordinary `change_level`
bindings remain supported.

In-game level loads freeze the exact last presented game view, with a small
static `LOADING...` card, while asynchronous preparation and GPU upload continue.
The window remains open and pumps events; Escape cancels and resumes the old
world. The cursor stays captured during this brief pause. Resize scales the
retained image. Startup keeps its normal preparation screen. Failure keeps the
previous world, reports the error and reopens a waiting elevator. Graphics-only
changes keep their existing loading policy.

Shaft movement occasionally sounds near the nearest landing (14–29 seconds
between later cues; an initial cue after eight seconds). Movement and arrival
also have quiet feedback. The preloaded spatial sources use a 1.5 m near distance
and fade to silence at 12 m, with the normal master/effects controls and occlusion.
The two new mono PCM WAV cues are original offline-authored assets under MIT;
recipes and hashes are in the audio authoring/provenance records.

## Saves, reset and validation

A checkpoint stores each elevator's settled local landing and open/closed state.
A save during door motion, travel or loading is refused with a useful message;
the previous valid save remains. Resume reconstructs the cabin and its controls
before accepting the player's pose. Reset returns the car to its authored initial
landing. An authored spawn inside a cabin resolves to that initial landing's deck.
Linked level travel follows the existing fresh-runtime-state contract and does
not transfer carried items or NPCs between levels. AI does not navigate elevator
shafts; passenger lift travel is for the player.

`projects/elevator-lab` is the small maintained demonstration: a two-floor lobby,
an Annex button, and a return lift. It is outside the release-map inventory.
Build it explicitly with Static lighting for mechanism acceptance:

```sh
places-compile export-project projects/elevator-lab \
  --out /tmp/elevator-lab.placesmap --variants static --asset-root assets --workers 4
places-compile validate /tmp/elevator-lab.placesmap --asset-root assets
```

Seven deterministic CPU regressions under `src/game/tests/elevators.rs` protect
walking/door collision, doorway reopening, physical floor transport at two frame
rates, single-shot loading after closure, safe save reconstruction destination
admission and upper-cabin spawning. They perform no map bake or GPU work. Native screenshots and trace
receipts establish visual, input, loading and audio behavior separately.

The explicit native regression campaign uses the direct game executable and a
fresh evidence directory. It never builds, bakes, wraps a Mac app, or touches an
existing installed map. The controlled failure renames only its own isolated
fixture installation. Allow about a minute for its four bounded cases:

```sh
python3 tools/bench/elevators.py --binary target/release/places \
  --package /tmp/elevator-lab.placesmap --out /tmp/new-elevator-evidence
```

The campaign asserts physical travel and return, upper-floor save/resume,
pixel-identical loading frames, resize, cancellation, full door reopening after
a failed load, and near/far sound attenuation. Native editor authoring can also
use the editor's ordinary `--verify-script` control/preview/apply/undo protocol.

Capture actual closed/open/interior/control views explicitly with `--mode views`:

```sh
python3 tools/bench/elevators.py --binary target/release/places \
  --package /tmp/elevator-lab.placesmap --out /tmp/elevator-views --mode views
```

[Editable art and rebuild instructions](../tools/props/elevator_70s/README.md)
describe the retained GIMP and Blender sources and fitted UV layout.

The native tool accepts `--lightmaps full` for a package explicitly prepared
with `--variants static,full`; this is useful for checking the moving cabin
practical alongside baked lighting on the small lab, without rebuilding the
release-map inventory.
