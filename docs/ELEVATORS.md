# Passenger elevators

Build > Elevator is the normal way to add a level-loading boundary or connect
rooms that should feel like different floors. The cabin has sliding doors,
a walkable interior, numbered destination buttons, warm lighting and occasional
spatial shaft-movement sounds. E or left click activates the aimed control.

The separate green door-arrow button below the destination buttons opens or
closes the cabin at its current floor. It never selects a destination. You can
close the doors to break a pursuing monster's sight without leaving the map or
floor. The prompt reads **Open doors** or **Close doors**; during travel it reads
**Doors locked during travel**. Pressing while a trip's doors are still closing
cancels that unstarted trip and reopens them. Presses during movement/loading
are ignored and are not queued. Occupants or props in the doorway still cause
safe reopening. A nearby monster can therefore prevent closure if it reaches
the opening before the doors seal.

Closed doors block the shared AI's vision and physical traversal. They do not
erase last-known-position memory or silence hearing: a monster may investigate
or wait outside. The baseline predator pursues the player but has no built-in
player damage/catch action; custom attacks must honor visibility/collision.

## Place, place, link

1. Make a hall with an opening for the elevator. Choose **Build > Elevator**
   beside Room, point at the floor at the centre of the doorway and click.
   **R** rotates the placement by 90 degrees. The preview shows its footprint.
2. Choose **Place another** and click the second location. Names are optional;
   use **Name** for labels such as G, 1 or 8.
3. Select the first cabin, choose **Link selected**, then click the second cabin.
   The link works in both directions. Repeat with a third cabin if needed.
4. Save with Cmd/Ctrl+S. Reopening retains the links. Undo reverses each placement,
   link or name change as one edit.

A group supports **three cabins, with two destination buttons per cabin** for
now. Two linked cabins show one destination button each; three show two, plus
the independent door button in either case. The current stop
is omitted. Linking existing groups would combine them, so the editor refuses
merges above the limit without changing either group. Independently linked
groups do not affect one another. There can be eight cabins in a map.

Floor labels and their order are independent of world height. Two linked halls
can sit side by side on the same plane, separated by a wall. Stops in adjacent
stacked buildings can form one apparent continuous building. The current
three-stop limit still applies. The engine closes the cabin, plays movement
feedback, then transfers the player to the linked cabin while the doors remain
opaque. It preserves position within the cabin and relative viewing direction.
It does not reload the map. A blocked or busy arrival reopens the source safely.

## Load another level

Place an arrival elevator in the other project map and **save that map first**.
Select the source cabin, choose **Load another level**, choose the project map
and its saved arrival elevator, then **Add destination**. The editor also adds
the allowed level transition. Configure a return destination in the other map
in the same way. A cabin supports two destinations in the simple flow, including
both local and separate-level destinations.

Save the maps, then use **Project > Export all levels + sources**. Play the
complete exported project for cross-level travel; embedded Test retains its
existing cross-level limitation. Existing ordinary level transitions remain
supported. Advanced configuration is optional and stays behind **Advanced**.

Loading starts only when the passenger is inside and the doors are fully closed.
The engine prepares and checks the destination cabin and arrival space before
replacing the old world. In-game loading retains the last presented game view
with a small static **LOADING...** card. The game window remains open and pumps
events; Escape cancels, resize scales the retained frame, and a failed load keeps
the previous world and reopens the source. Startup retains its normal loading
screen. Arrival opens the destination doors automatically.

The rider retains their exact cabin-relative feet position, viewing direction,
pitch and crouch across a level load, including rotated or elevated arrival
cabins. A blocked exact position rejects the trip instead of moving the player
to the cabin centre. Held controls remain usable while carrying a prop.

## Bring props and NPCs

Ordinary **authored movable rigid props and NPCs** travel automatically when
their complete bodies fit inside the closed cabin. This works between linked
cabins in one map and between project levels. Objects outside stay behind.
Held props must fit completely too. Position, orientation, rigid velocities,
sleep/dormant state, held ownership, ordinary component state and NPC intent
are retained in the destination cabin's frame. Navigation replans there; NPC
references to the player or fellow passengers remain semantic identities.
An NPC's old post outside the cabin is replaced by its arrival post.

Travelled objects have one owner across visits. Returning to their original
map does not recreate a second source instance. The session ledger, including
travellers left behind in a visited level, is saved with checkpoints. It holds
up to 32 distinct travellers; loading an unrelated package starts a new ledger.
An obstructed arrival, missing level or cancellation retains the source world,
occupants and held item, and reopens the doors.

The current portable subset excludes console/session-created props, runtime
spawns, ragdolls/articulated actors, NPC-held assemblies, lifetime/balloon owners,
and actors with map-bound tasks, regions, wiring, lights or effects. Such an
occupant refuses travel with an explanation. Static scenery is never cargo.
Use linked cabins for prop/NPC transport; advanced physically moving shafts
currently accept the player alone and refuse occupied trips before movement.

## Space and artwork

The placement origin is the door threshold centre. At yaw zero the hall is
along -Z and the cabin extends along +Z. Reserve a **3 m wide front pocket
assembly**, a clear **2.6 m wide, 2.5 m deep shaft** behind it, and a height of
at least 2.75 m. The usable cabin is 2.4 m wide, 2.4 m deep and 2.55 m high;
the clear doorway is 1.35 m wide and 2.2 m high. Use a 1.5 m wide opening at
least 2.25 m high in a landing wall at local Z=0..0.2. The elevator does not
cut walls, room floors or ceilings for you. Build/export validates clearance.

Satin gray leaves retract into hollow dark recessed pockets, retaining a narrow
visible panel lip at full opening. Concealed top/side overlaps and a rear centre
seal hide the cabin and outside scenery when closed. The inner pair sits behind
the landing wall's cabin-facing plane, so stationary walls cannot clip into the
view during travel. Render and collision share the same eased door pose.
An occupied doorway reopens closing doors and cancels departure.
The inner metal stop frame overlaps the closed leaves, with a small visible
leaf edge beside the dark pocket when open. The opaque 1.72 m sill spans both
jambs and both door tracks, projecting from Z=-0.32 to +0.42 m. Recessed grooves
have backing, so underlying carpet cannot show around or through the threshold.
The doorway's jambs, pocket folds, reveals and header are metal throughout;
the surrounding front wall and wall above the doorway remain walnut, matching
the side/rear cabin panels. Dark metal liners preserve the recessed pockets.

The reference-inspired exterior surrounds an original 1970s office interior:
walnut paneling, wood rails on brass brackets, muted linoleum, cream ceiling and
a warm diffuser. GIMP/XCF and Blender sources, committed PNGs and four rigid GLBs
remain editable. No texture imagery is generated at runtime. The warm attached
light uses the existing bounded realtime-light budget; the diffuser also emits.
Cabin light is confined by the opaque shell and both sliding door tracks. The
actual eased door aperture admits the warm pool only as the leaves separate;
closed doors, centre seals, jambs, header, deck and cabin walls block it. The
interior practical stays on during closure and travel. This visibility is shared
by Static/Full lighting, world surfaces, props, decals and reflection captures;
it requires no runtime bake or additional shadow-map pass.
See the [art workflow](../tools/props/elevator_70s/README.md).

The explicit dark-room fixture is `tests/fixtures/levels/elevator_dark.json`.
Prepare just that scene with Static/Full, then use
`tools/bench/elevator_light.py --binary <direct-game-binary> --package <fixture.placesmap> --out <new-directory> --assert-sealed`
for closed/partial/open/closing captures from the hall, cabin and shell sides.
Each image has an authoritative elevator-state checkpoint and measured exterior
floor pixels. `--lightmaps off` checks Static; `--bloom` checks the normal luminous
composition. The tool is opt-in;
ordinary tests and CI never bake this scene or launch a native window.

Shaft ambience occurs occasionally near a landing: an initial cue after eight
seconds, then 14–29 seconds between cues. Movement and arrival have quiet feedback.
Spatial sound uses a 1.5 m near distance and fades to silence at 12 m, through the
normal master/effects controls and occlusion. The original offline-authored WAV
cues retain their audio provenance records.

## Advanced physical shafts and saves

Advanced preserves the existing physical shaft mode: one cabin moves vertically
between local landing elevations, accelerating and decelerating within speed_mps.
Physical landings must be separated by at least 2.75 m and share a clear shaft.
The data retains capacity for eight physical stops; linked cabins and loading
routes use the initial three-stop/two-destination limit. In source data, a stop
without level or arrival_elevator is a physical landing. arrival_elevator alone
references another cabin in the current map; level plus arrival_elevator targets
a saved cabin in another map. initial_stop names a physical landing.

Saves store each cabin's settled landing and open/closed state. Saving during
motion or loading is refused while retaining the previous save. Resume rebuilds
controls before restoring the passenger. Reset returns cabins to their initial
landings. An authored cabin spawn resolves to the initial deck. NPCs can walk
through open landing doors using prepared navigation; runtime door collision
still gates closed cabins. Navigation is prepared offline, never during play.

## Temporary playable demo

projects/elevator-links is intentionally temporary and excluded from shipped-map
preparation. Its beige Ground hall and blue Floor 1 hall are **on the same plane**,
10 m apart. G/1 buttons move between them without reloading. A loads the separate
Annex, whose G/1 buttons return to either hall. All routes are playable both ways.
An ordinary NPC and a pickup box start inside Ground's cabin, demonstrating
automatic passenger transfer. Their transport requires no extra editor setup.

Export just this small fixture and launch the game executable directly:

```sh
places-compile export-project projects/elevator-links \
  --out /tmp/elevator-demo.placesmap --variants static,full --asset-root assets --workers 2
python3 tools/bench/elevator_demo.py --play --binary target/release/places \
  --package /tmp/elevator-demo.placesmap --out local/elevator-demo-play
```

The isolated playtest folder retains its saves/settings. No Mac app bundle is
used. The older projects/elevator-lab remains the physical-shaft regression.

## Explicit validation and recording

Passenger and exact-rider acceptance are opt-in and use tiny fixtures, separate
from ordinary Cargo tests. Generate sources without baking, export the chosen
project explicitly, then run the direct binary:

```sh
python3 tools/bench/elevator_cargo_fixture.py --out local/cargo-fixtures
places-compile export-project local/cargo-fixtures/straight \
  --out local/cargo.placesmap --variants static,full --asset-root assets --workers 2
python3 tools/bench/elevator_cargo.py --binary target/release/places \
  --package local/cargo.placesmap --out local/cargo-held --mode held
python3 tools/bench/elevator_rider.py --binary target/release/places \
  --package local/cargo.placesmap --out local/rider-offsets
```

Export `rotated` and pass `--rotated` to check a 90-degree cabin 3 m higher.
The fixture generator also writes blocked rider/cargo/NPC destinations and a
physical-shaft refusal case. Use `--mode blocked-local`, `blocked-level`,
`failure`, `cancel` or `shaft-guard` with the corresponding package. The driver
checks live recovery input and source ownership; it never changes installed maps.

CPU regressions cover collision/reopening, physical and linked travel,
independent groups, same-height placement, group limits, undo/save/reopen,
arrival admission and save restoration. They do not bake maps. Native tools
are opt-in and use disposable state. No shipped-map bake is required:

```sh
python3 tools/props/elevator_70s/check_seals.py
python3 tools/props/elevator_70s/check_surface_joins.py
python3 tools/bench/elevator_links.py --mode demo --out /tmp/new-editor-evidence --run
python3 tools/bench/elevators.py --binary target/release/places \
  --package /tmp/elevator-lab.placesmap --out /tmp/new-native-evidence --lightmaps full
```

The physical-lab campaign verifies both directions, collision, saves, identical
frozen loading frames, resize, cancel/failure reopening and sound falloff.
Use --mode views or --mode seals for visual checks. The separate --mode links
uses the groups fixture produced by elevator_links.py --mode groups, covering
same-height travel, adjacent stacked rooms and an independent second group.

Omit --play from elevator_demo.py and use a new output folder to capture its
actual native renderer frames and game audio. FFmpeg is required; capture timing
is retained when muxing. Recording samples every rendered frame before timestamp-
correct 30 fps delivery, with continuous walking and eased camera turns after
one initial acceptance pose. The temporary package is versioned 1.0.1 so it can
replace the earlier 1.0.0 demo after the mechanism artwork changes.
The tutorial uses captures of the actual native editor
place/link, destination chooser, save and complete-project export workflow.

The fitted artwork keeps uniform charcoal/rubber stock on a committed centre
texel and crops elongated faces according to their physical aspect ratio. This
prevents adjacent atlas colours from appearing on thin pocket, metal, wood and
floor edges at grazing views. It changes UVs, not door geometry or motion.
Previously exported elevator maps must be rebuilt against updated mechanism
assets; installation admission rejects a package with old dependency hashes.

To inspect the actual interior opening animation, capture every rendered frame
losslessly with an explicitly prepared, current lab package:

```sh
python3 tools/bench/elevator_motion.py --binary target/release/places \
  --package /tmp/elevator-lab.placesmap --out /tmp/new-motion-evidence \
  --view left --arrival --lightmaps full --filtering high
python3 tools/props/elevator_70s/check_uv_stability.py
```

The motion tool retains FFV1 footage, individual native PNG frames, timestamps
and door-pose checkpoints. Frame timestamps are authoritative; the FFV1 stream's
nominal 60 fps is not a performance measurement. Compare center, near/far,
oblique, close-oblique, lintel and sill views; both lighting modes and filtering
extremes matter. Albedo inspection separates texture sampling from lighting. --arrival remains
inside through ordinary floor travel and the complete arrival opening; optional
--bloom, --reflections full and --fov cover normal presentation settings.
The CPU UV guard ignores microscopic Boolean-weld slivers below 0.1 mm², whose
UV derivative is ill-conditioned; it does not replace native motion inspection.
