# Editor authoring and safety (0.12.0 task5)

Run `places-editor` beside `places-compile` and `places`, with the external
`assets/` payload described in [INSTALLATION.md](INSTALLATION.md). No map bake is
required to compile any application.

```sh
/path/to/places-editor /path/to/project
/path/to/places-editor --new /existing/parent/my-room
/path/to/places-editor /path/to/project --level entry \
  --compiler /path/to/places-compile --game /path/to/places
```

All tools share installation/user-path resolution and preserve the launch
directory. `--asset-root`, `PLACES_ASSET_ROOT`, `PLACES_COMPILER`, and
`PLACES_GAME` support explicit installations. Compiler/game paths are independent.
New chooses a parent folder and creates a unique child; Open chooses a project
folder. The default is the user's `projects/` folder. `--new` requires a new path.
The starter contains a six-metre floor/ceiling and a built-in light.

| Control | Action |
| --- | --- |
| Left click geometry or a scene row | Select; outlined bounds/axes show selection |
| Right drag / middle drag / wheel | Orbit / pan / zoom |
| Hold right mouse + W/A/S/D/Q/E | Fly forward/left/back/right/down/up |
| F or Frame | Frame selection, or the scene if nothing is selected |
| Numeric fields / adjacent minus and plus | Set position, dimensions, yaw, prop pitch/roll and uniform scale |
| Arrows / Page Up, Page Down | Move selection on X/Z / Y |
| R / plus, minus | Rotate / scale selection |
| Grid / Snap / Step | Show metre grid, enable snapping, cycle transform step |
| Cmd/Ctrl S, Z, Shift Z (or Y), D | Save, undo, redo, duplicate |
| Delete / Backspace | Delete, provided semantic references remain valid |
| Cmd/Ctrl N, O, B, P | New, open, save+build, build+play |
| Esc | Cancel numeric entry or close a popup |

Click a value, use Cmd/Ctrl A to clear it, Enter to apply, Esc to cancel.
Rooms/walls use minimum-corner coordinates and axis-aligned footprints. Their
quarter-turn rotation swaps dimensions about the centre and displays zero yaw
afterwards. Unsafe rotations of profiled/per-face/opening data are refused.
Prop Y is floor-relative; all scale fields change uniform scale. Prop library
chooses the next built-in model; Material assigns a floor/wall material. Scroll
the scene list independently. The shared renderer applies compiler trim/fixture preparation, cuts away ceilings
and uses neutral preview light; Play shows authored ceilings, lighting and dynamic behavior.

Save validates the raw authoritative scene and uses an owned sibling temporary
plus atomic rename. Unknown fields remain intact; the manifest remains byte-for-byte.
Unsupported versions, duplicate keys, numbers that cannot be preserved exactly,
external source/manifest changes and unsafe paths are refused. Per-level histories
and dirty states survive switching. Close/New/Open with unsaved changes offers
Save, Discard, Cancel; failed saves retain edits. Save-all preflights visited
sources; an I/O failure may leave an already saved subset, each atomically.

Save+Build saves all dirty levels and compiles the selected source to its sibling
`.placesmap`. One job runs at a time. Save is unavailable during compilation;
further edits remain in memory. Progress reports actual current-pass work,
indeterminate when uncounted. Cancel requests safe cooperative shutdown with a
bounded kill/reap fallback. Failure/cancellation retains the last usable package.
Finish/cancel builds or stop play before switching levels/projects or closing.

Build+Play saves/compiles, imports through the direct game executable and launches
the exact level ID. Profiles under `cache/editor-play/` isolate each package/level
from ordinary installed maps/settings and other projects' output filenames. Quit
the game or click Stop Play to return to editing. Bundled map ID conflicts are
reported by the existing importer; choose a distinct project level ID.

Compiler `build --events --progress off` emits versioned NDJSON `started`,
`progress`, `finished` events on stdout. Final outcomes are `success`, `failed`,
`cancelled`; success carries `BuildReport`. Diagnostics use stderr. Keep stdin
open while building; send `{"command":"cancel"}` plus newline, or close stdin,
to cancel. Neither UI nor event consumers scrape human terminal output.

For developer evidence, `--capture-dir /existing/directory` enables F12 numbered
PNG framebuffer captures without overwriting existing files. These are evidence,
never map assets. See the task3 report for native validation.
`--verify-script /absolute/script.json` runs an explicit bounded native acceptance
sequence through the ordinary editor event handlers, with actual framebuffer
captures and document/state checkpoints. This is developer tooling, inactive
during normal editing; the task3 evidence retains the scripts used. It permits
repeatable checks of unbundled direct executables when desktop automation cannot
address their windows.

`+ Geometry` groups floor/ceiling, wall, ramp and recess creation; `+ Prop` keeps
model placement direct. Detailed controls follow the selection. Ramp Offset is
relative to the carrier room floor; Rise is signed toward the footprint's longer
axis, shown by UP +/-X/Z. Width/Depth choose the run axis (ties select X). Turn
uses quarter turns and preserves the two endpoint heights; square quarter turns
are refused because the format cannot express their changed axis. Recess Offset
is signed; negative lowers the basin and positive raises a platform. A region
has no independent thickness. Water remains an independent preserved collection.

Select a floor, then `Shape >` to choose a snapped world-space rectangular
opening or add an upper floor. A cut replaces the selected room with up to four
strips and removes **both its floor and its ceiling** inside the rectangle.
It preserves materials, tile frames and unknown record data in each strip.
Other storeys remain; a through shaft requires explicit cuts on each storey.
The upper-floor tool copies the footprint one room height above, retaining the
lower ceiling. Gable splits, occupied upper space, and ambiguous global walking
modifiers are refused. Connect storeys through a separate shaft/landings; ramps
and regions affect every overlapping room footprint and ramps cannot span
different room floor planes. These operations are one undo step each.

`Safety >` runs a bounded background review of the current revision. Findings
separate void/drop and player-collision risks, geometry overlap, duplicate or
coplanar sheets, and material/depth uncertainty. The editor reuses the production
prepared-geometry checker, samples room/ramp edges at the player's crouched jump
apex, checks deep-basin exits, and compares actual transformed static GLB
triangles with architecture and other models. Colored bounds localize risks;
measured triangle pairs are outlined when available. `View in viewport` closes
the details; reopening Safety retains the reviewed finding. Model collision
warnings and envelope layering estimates are labeled estimates, not triangle
proofs. Intentional artistic intersections remain unchanged; decals use their
separate depth-biased pass and are excluded from model coplanarity comparisons.

Only identical simple raw wall records receive a removal candidate. Metadata,
openings, per-face data and props disqualify automatic deletion. `Preview fix`
renders the candidate without editing source/history and leaves the scene open
to camera navigation. `Back to finding`, Close or Esc restores the source view.
`Apply` is available after preview, applies one correction and creates one undo
step. Any intervening source edit invalidates the candidate; validation failure
leaves the source and history untouched. No bulk correction or automatic filling,
prop movement, intersection repair or artistic reinterpretation is performed.

The checks are not an exhaustive no-clip, watertightness, reachability or rendered
z-fighting proof. Edge sampling can miss narrow routes; motion, alpha pixels,
water, ladders and every camera/depth situation are not simulated. Oversized
scenes, missing models and exhausted budgets produce explicit incomplete-review
findings. Play-test jumps, shafts and pools. Source and manifest formats remain
unchanged; geometry revision 3 requires recompiling prior prepared output for
full prop orientation and the preceding ramp correction. See [stage 4 evidence and limits](reports/editor-0.12.0-task4.md).

`Ignore for this placement` dismisses only the selected warning and its measured
instance/geometry context. Save persists it; normal finding navigation skips
saved decisions. `Review saved` shows active and stale decisions, with
`Clear ignore` restoring normal review and correction eligibility. These edits
support Undo. Other warnings on the same item and other placements remain
independent. A meaningful placement or affected-geometry edit reactivates the
warning. Unknown-size legacy props and unsupported geometry deliberately use
conservative contexts, which may reactivate a decision more broadly.

Every correction gateway, including the shared batch helper, skips ignored
issues and protects their placements/support context. An unrelated correction
that would affect protected context is refused for explicit review. Clearing a
decision is a separate authoring action; source validity checks always remain
active. Metadata details are in [PROJECT_FORMAT.md](PROJECT_FORMAT.md).

`+ Environment / gameplay >` opens one focused creation sheet for built-in light
fixtures, ambient/positional sounds, entities, closed doors, triggers, regional
fog, ground fog and entity spawn points. Objects share the scene list, viewport
selection, numeric transforms and undo history. `Configure >` opens the selected
object's capability sheet; the viewport remains the central workspace when the
sheet closes. The model row of a prop opens the built-in model library and sets
the model for the selected placement and the next `+ Prop`.

Lights expose fixture family, illumination power/range/colour, separate visible
face emission, enabled state and runtime switching. `Ceiling / wall` selects a
compatible built-in housing. With Snap enabled, newly placed fixtures and props
mount against their real carrier using the fixture back or transformed model
bounds. `Mount surface` repeats this explicitly; manual position and rotation
remain available. Prop Y stays floor-relative and permits negative offsets;
yaw, pitch and roll preserve deliberate embedded or sideways scenery through
save/build/play. Snapping never continuously pulls an edited placement back out
of an intentional intersection.

Sound emitters have no runtime model. Choose an installed cue, gain, looping,
play-on-load and enabled state. `Positional` uses the emitter's world position;
turn it off for level-wide ambience. A cue's ambient/positional policy can be
overridden per emitter. Triggers can start a configured emitter, including its
loop policy. Sounds use the existing preloaded shared audio backend in Play.

A prop's `Physics` toggle opts only that placement into the shared rigid-body
system. Pickup, mass and solidity remain contextual controls. Movable items load
frozen and wake when interacted with or activated by a trigger; enabling physics
assigns a reusable activation group when needed. Entity `Behavior >` configures
the existing shared idle/wander/follow/flee components and navigation agent.
Movement uses the compiler's navigation bake; choose Play to check paths and
collisions. No behavior is embedded in the editor or tied to a specific model.

Doors use hinge coordinates, width/height/thickness and closed-leaf yaw. Place a
leaf in an existing wall opening; creation defaults closed with an E interaction
binding. The capability sheet exposes two-way opening, locked state, swing angle
and speed. Two-way opening chooses the side away from the interacting actor;
targeted trigger actions use the same shared door dispatcher.

Triggers are resizable axis-aligned boxes. Their translucent guides are visible
only in the editor. `Properties` selects player, any physical item, held/unheld
items, an exact item instance or an exact entity identity, per-actor or aggregate
occupancy, enter/exit event, one-shot policy and cooldown. `Actions >` lists the
ordered typed actions and permits individual removal and addition: play an
emitter, open a door, spawn at a point or activate a physics group. The shared
runtime validates references and enforces action/occupancy budgets.
Configure policy before adding actions if preferred; an empty trigger keeps a
validated editor draft and produces no runtime action until an action is added.

Entity spawn points are visible movable markers, separate from live placements.
Choose a built-in model/behavior and bounded lifetime in their reusable template,
or select an existing template. Duplicated points share the template and receive
independent one-active-spawn groups. Editing a shared template affects its points.
A trigger's spawn action names the point; the ordinary shared entity runtime
creates and expires the instance. This wiring is reusable by the upcoming Luau
and game-mode work.

Regional fog fills a bounding box; ground fog independently fades upward from
its authored base. Transform sets their footprint and height. Configure sets
extinction density, colour and horizontal edge softness. `Preview: Lighting`
uses authored vertex illumination and atmosphere in the editor cutaway;
`Preview: Inspect` uses neutral inspection lighting. Play shows the compiled
lighting and full ceiling. Fog and trigger guides never enter runtime geometry.

Explicit mounting supports flat ceilings, axis-aligned solid wall faces and
continuous floors/ramps. A gabled/open ceiling, incompatible fixture family or
footprint crossing a step is refused without moving the object. Fixtures retain
their existing yaw-only mounting contract. Fitted spans/floating props retain
their own upright poses; tilted finite-area attached lights retain horizontal
emitter footprints. Manual transforms and intentional solid intersections remain
available. Full transport uses actual prop triangles; Static collision/occlusion
retains conservative transformed envelopes.

Custom importers, Luau, project bundles, menu scenes and final distribution
signoff remain later work. Model integration is deferred until stages 4–7 are
complete. Other unsupported source data remains preserved. Native Windows/Linux
acceptance remains outstanding.
