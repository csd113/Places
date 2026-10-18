# Editor authoring and safety (0.12.0 stage 7)

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
| Tab / Shift Tab, Enter | Focus next/previous visible control, activate focused control |

Click a value, use Cmd/Ctrl A to clear it, Enter to apply, Esc to cancel.
Rooms/walls use minimum-corner coordinates and axis-aligned footprints. Their
quarter-turn rotation swaps dimensions about the centre and displays zero yaw
afterwards. Unsafe rotations of profiled/per-face/opening data are refused.
Prop Y is floor-relative; all scale fields change uniform scale. Prop library
chooses the next built-in model; Material assigns a floor/wall material. Scroll
the scene list independently. The shared renderer applies compiler trim/fixture preparation, cuts away ceilings
and uses neutral preview light; Play shows authored ceilings, lighting and dynamic behavior.

Save validates the raw authoritative scene and uses an owned sibling temporary
plus atomic rename. Unknown fields remain intact; the manifest remains byte-for-byte during scene saves. Explicit Project metadata operations save edits first and atomically publish the changed manifest.
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

Build+Play saves and exports the complete project, imports through the direct game executable and launches
the selected level ID. Authored transitions stay within that package and create fresh runtime state at each destination. Profiles under `cache/editor-play/` isolate each package/level
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
bounds, with 2 mm separation outside the geometry checker's coplanarity tolerance.
`Mount surface` repeats this explicitly; manual position and rotation
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
creates and expires the instance. This wiring is reused by Luau and future game-mode work.

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

Custom runtime asset importers, menu scenes and final distribution
signoff remain later work. Model integration is deferred until stages 4–7 are
complete. Other unsupported source data remains preserved. Native Windows/Linux
acceptance remains outstanding.

## Luau source workflow

`Scripts >` opens the selected map’s modules. Create/open/edit source, Save,
Check and inspect bounded Diagnostics in one focused sheet. Select a trigger,
choose Actions / Script handler, open a module and Link handler. Enter/exit,
once, cooldown, actor filters and ordinary action removal remain the trigger
controls. Build+Play snapshots each map’s scripts into the complete project; replay/reset recreates
VM state. No external editor is required. See [the API, limits and lifecycle](SCRIPTING.md)
and [the native stage 6 evidence](reports/editor-0.12.0-task6.md).

## Complete projects and source recovery (stage 7)

`Project >` groups name/author/description, adding a level by stable ID, switching
the selected level, using it as the entry, attaching a selected-level thumbnail,
and allowed transition destinations. The existing New/Open controls remain.
Metadata changes save current edits first. Level membership/manifest metadata
changes publish immediately; scene transitions use the ordinary level history.
There is no delete/rename-level command that could break authored references.

Attach thumbnail takes an existing PNG file path and copies validated bytes into
`thumbnails/<sha256>.png`, then links that file. No pixels are cropped or resized.
Use a landscape 16:9 RGB8/RGBA8 PNG, maximum 1280×720 and 4 MiB. The Project sheet
shows its reference. Packages distribute/recover the original image and metadata;
the player still uses its simple text level list, not thumbnail cards (#6).
Custom texture/model/sound importing remains unsupported and has no placeholder UI.

Export all levels + sources takes an explicit `.placesmap` path. It runs the same
compiler/progress/cancellation job slot as ordinary builds, then shares identical
records by hash and publishes one file. An existing output must belong to this
project and remain unchanged through compilation. New output, failure and cancel
paths preserve existing user work. Save+Build still compiles only the selected
level; a map with transitions needs complete Export/Build+Play for playback.

`Project >` is available in an empty editor, so recovery needs no dummy project.
Recover editable project takes a project package and a **new** destination folder.
It validates the complete archive, every authored source, script and thumbnail,
then publishes the folder atomically without replacing any existing path. A
successful recovery opens the source project. A normal single-map compiled package
has no editable recovery payload and is refused with an explanation. The player’s
Import action installs compiled data for playing; it does not recover authoring files.

Check project and Migration check validate all declared version-1 source data and
bounded script initialization. Version 1 needs no conversion. Unsupported versions
are refused without writing; no legacy conversion is currently implemented.
A migration check never claims that it upgraded a file.

Allowed transitions is a comma-separated list of declared project level IDs for
this selected level. Trigger Actions / Change project level adds an ordinary
`change_level` action and declares its destination. Scripts may use
`api.change_level(id)` only for a destination in this list; `api.reset()` uses the
existing reset-to-start action. First navigation request per frame wins. Reset
cancels pending navigation. A transition loads only its destination, at its authored
spawn, with fresh VMs, world actions and player state; campaign progress persistence
is not implemented. See [project/package contracts](PROJECT_FORMAT.md).

## Navigation inspection

Open `Navigation >` for the optional inspection panel. `Bake current source`
uses the production navigation compiler on the current document snapshot, without
building lighting or writing a package. Any source change, undo or level change
hides stale coverage and routes until another explicit bake.

Cycle through baked bodies or authored actors; the panel shows radius, height,
step limit and door capability. Coverage colors connected regions and marks
clearance/door refusals red. Use the height slice to inspect overlapping floors,
or show all floors. The displayed cell count makes the 12,000-cell overlay cap
visible. The underlying query still uses the complete baked graph.

`Start at agent` uses the selected actor or spawn point's authored position.
For a class-only entry, `Pick start` and `Pick goal` select floor cells
in the viewport. Green/pink endpoint markers and the route remain in the scene.
Feedback distinguishes invalid endpoints, disconnected/unreachable routes,
complete routes and budget-limited partial routes; it also reports expansions.
The inspection uses authored door state. Moving props and actor avoidance must
be verified in Play, where current physics and actor bodies are available.

### Readable native editor UI (0.12.0)

The scene's **Find** control filters object names/identities. Empty text restores
the full list. **Pose** stages a complete transform: edit several fields, Preview,
then Apply once. Preview does not mutate the document; Cancel restores the scene.
A refused Add clears selection so later transforms cannot edit an old object.
Prop Y is an offset above the first local floor; it does not imply upper-storey
ownership. Room cuts still remove both floor and ceiling.

**Configure → Capabilities** groups identity/interaction, typed state, events,
light circuits, physics details, AI/beam response and attached audio. Forms show
six rows with pagination. Events select the exact binding before editing its
conditions/actions or linking Luau; adding an exit event preserves the enter
event. Trigger identity and held policy are independent, and receipt can use
overlap, centre, or settled centre. Existing source extensions survive edits.
Instance/state renaming updates structured references and refuses maps with
Luau modules, whose dynamically constructed references cannot be safely guessed.

Physics details expose the reset group and independent standable dimensions.
**Use fitted catalog shape** installs an existing bounded fixed profile;
arbitrary hull/compound construction remains source tooling. The sound chooser's
**Listen** control plays one preloaded cue quietly for at most three seconds.
**Project → Map settings** exposes spawn, default materials, global presentation,
fog and the explicit Full-lighting requirement. Required Full maps refuse lesser
lighting selections with a named message; no runtime bake occurs.

The editor requests the native high-density drawable and uses an independent,
committed antialiased Roboto Mono atlas. Text/buttons scale uniformly with the
window; world texture quality does not affect UI text. The retained slate/mint
workspace groups project/distribution controls in Project, scene creation and
transforms in the left column, and build/view controls across the toolbar.
A mint outline identifies keyboard focus; numeric and script entry retain their
normal editing keys. Project → Map crosshair toggles the selected map's default
through normal undo/save history. Game `/crosshair help` documents overrides.

Project → Level menu image offers Choose PNG, Paste path, preview and Remove.
Use a 16:9 RGB/RGBA8 PNG up to 1280×720 and 4 MiB. The validated file is copied
under a content hash into `thumbnails/` and embedded unchanged in project export.
Per-map images appear before names in Explore maps. Only the entry map inherits
a package thumbnail; other maps use their own image or the neutral P fallback.
Removing an entry image also clears that package fallback, while leaving shared
PNG files intact. Browsing decodes only bounded visible images, not level worlds.

Entity Configure includes **Shadow only**, independent of **Solid** and AI.
It retains the posed model solely for real handheld-flashlight shadow casting;
Play hides the colour/reflection/emission mesh. Inspect preview keeps the authoring mesh selectable; Environment preview applies
the runtime policy. Spawn-point Configure applies the flag to its shared template.
Undo/save/reopen retain it. Use a supported animated model; unsupported or
character-budget-exceeded placements fail closed with a diagnostic. See the
[map contract](MAP_AUTHORING_GUIDE.md#shadow-only-entities-0120).


## Relay Annex authoring tools

`Geometry > Space and environment` groups complete-record forms for ramps,
stairs, guardrails, water, sky, snow/weather, directional sources, the selected
room ceiling, selected-floor overrides, explicit GLOBAL patches/regions and legacy actor routes. Select a room
before changing its ceiling or making an owned floor section. The form's record
number addresses its collection; previous/next and New choose records. Blank
optional scalar fields keep engine defaults. Vector components require numbers.
Preview validates the complete candidate; Apply enters one undo step. Escape or
Cancel restores the scene. Unchanged and unknown source fields remain intact.

Add Ramp opens a creation draft before validating the default pose. Enter its
complete destination and dimensions, then Preview/Apply. A bad default target
therefore cannot strand the author without transform controls. Staircases use the
longer footprint axis as the run; landings remain ordinary room sections. Rails
have explicit world base Y and optional rise, or follow the floor when omitted.

A selected-floor override splits only that room into owned rectangular sections,
keeps its ceiling plane and adds short boundary faces for an elevation change.
Other storeys retain their materials/elevations. Existing overlapping global
regions/patches, slopes and ambiguous same-plane carriers are refused explicitly;
use the GLOBAL forms to remove/rework those records first. Their warning explicitly
identifies the effect on every overlapping storey. This uses ordinary level geometry
and does not
change legacy global patch/region semantics. Water preview includes X/Z elevation
slices with real room/ramp/stair samples and the selected water depth band.

Legacy route forms preserve ordered existing steps and offer Move, Wait and Pose
steps. Field labels identify step numbers; pages keep the sheet compact. The
validated path preview is sampled for display; admission retains the finer
collision/support check. Walking support follows the actor's current storey and
cannot silently fall to another floor. Navigation AI remains its separate system.

Safety includes exact dormant-item shape penetration candidates. Resting contact
within 2 mm is excluded; artistic placements receive no automatic movement.
Play's developer `/trigger show` also displays the nearest authored item's
actual shape centre and held/dormant/awake/sleeping state, plus nearby receiver
bands. Green means eligible occupancy under the real receipt/filter policy;
script conditions and completion remain separate. Ordinary gameplay warns after
a blocked release or sustained held-motion refusal. The overlay is opt-in.

Project > Selected map title stores bounded `display_name` metadata. Inactive
menu rows read that metadata without loading inactive world semantics. Empty
titles retain stable-ID fallback. Guidance wraps authored paragraphs and splits
long words; Page Up/Page Down browse six-line pages; Enter/Escape retain dismissal.

For explicit CPU latency checks, run `editor-profile PATH_TO_ISOLATED_PROJECT`.
It reports document operations and performs reversible in-memory transform/history
work. Native `--verify-script` recipes retain per-action handler and idle-wait
milliseconds in `timings.jsonl`. Neither is an FPS benchmark or a CI map bake.
