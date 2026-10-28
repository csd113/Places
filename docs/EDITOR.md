# Native editor authoring and safety (0.13.1 development)

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
The starter contains a six-metre room and a built-in light.

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
Prop Y is relative to the room base; all scale fields change uniform scale. Prop library
chooses the next built-in model; Material assigns a surface/wall material. Scroll
the scene list independently. The shared renderer applies compiler trim/fixture preparation, hides ceiling faces when viewed from above and shows their textured underside
when looking up from inside. The real ceiling mesh, collision, bake and exported
world remain intact. Inspect uses neutral light; Play shows authored lighting
and dynamic behavior.

Save validates the raw authoritative scene and uses an owned sibling temporary
plus atomic rename. Unknown fields remain intact; the manifest remains byte-for-byte during scene saves. Explicit Project metadata operations save edits first and atomically publish the changed manifest.
Unsupported versions, duplicate keys, numbers that cannot be preserved exactly,
external source/manifest changes and unsafe paths are refused. Per-level histories
and dirty states survive switching. Close/New/Open with unsaved changes offers
Save, Discard, Cancel; failed saves retain edits. Save-all preflights visited
sources; an I/O failure may leave an already saved subset, each atomically.

Save+Build saves all dirty levels and compiles the selected source to its sibling
`.placesmap`. One job runs at a time. Save is unavailable during compilation;
further edits remain in memory. The footer shows a real **current-pass** progress
bar, current source, lighting variant and stage. The exact completed/total count
and unit belong to that pass; a full pass bar does not mean the whole bake or
export is complete. Uncounted stages use an animated track explicitly labelled
indeterminate. No whole-job percentage or ETA is estimated. Success appears only
after the compiler confirms publication and exits successfully.

The scope line distinguishes the selected level (including a stale-row build,
which keeps other level outputs) from complete-project export, validation and
source recovery. Complete-project operations identify each current member in the
stage line. Feedback and Cancel remain visible above editor popups. Cancel requests
safe cooperative shutdown with a bounded kill/reap fallback; the job stays active
until the compiler is reaped. Failure/cancellation retains the last usable package.
A selected-level output with project transitions is a prepared member, not a
standalone runnable distribution. Validate/play it in the complete project through
Export or Build+Play; other member outputs need not be rebuilt for a stale-row bake.
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

Simple mode exposes **New Room**, **Add Wall**, **Add Door** and **Add Light**
beside the viewport. New Room reuses the adjacent-room draft: Preview and Apply
accept its room-height defaults, then the gold edge handles stretch it in place.
Advanced mode retains build, scripts, navigation and precise property sheets.

**Add Wall**: click the start on a room floor, move the pointer to preview, then
click the end. The segment follows X or Z and defaults to 0.20m thickness and the
room's ceiling profile. Snap uses the toolbar step and nearby wall endpoints.
Enable **Chain** to keep clicking connected segments; Escape ends the unfinished
segment. Completed segments each have their own undo step. Click a wall to select
it, then drag its body across the floor, or drag its mint Start/End handles to
lengthen or shorten it. The opposite endpoint remains fixed. Axis handles and
Move / exact size remain available for precise edits.

**Make Doorway** on a selected wall cuts an opening only. **Add Door** fits an
interactive leaf into an existing doorway, or creates the opening and leaf as one
undo step. Point at the intended wall face; the preview shows the result before
the click. **Add Light** places a ceiling fixture above the clicked room floor.
Escape or focus loss cancels a placement/drag. Use the Add action again to repeat.

Wall edits retain automatic trim, carry nearby leaves, wall lights and decals,
and rebase opening offsets when moving the start endpoint. Shrinking through an
opening, severing a wall junction or moving an ambiguous elevated fixed prop is
refused with an explanation. Room edge stretching keeps its existing attachment,
shared-boundary and support checks. Trim is regenerated by the shared preparation
path; these previews do not alter source or create history steps.

**Layers / focus** hides architecture, props, lights or gameplay guides.
Isolation keeps a room and its contents, or a wall and its nearby fittings, with
floor/ceiling carriers for context. Visibility/isolation is session-only; Show all
restores it. Selected-object locks persist through save/reopen and undo/redo.
Locked objects can still be selected/framed, and block indirect attachment edits.

The material picker defaults to **24 small tiles in four rows**; Tiles: Medium
shows 15 in three rows. Search, theme filtering, paging, keyboard focus and the
current assignment remain visible. The missing Backrooms swatches use the same
committed 160x120 texture-and-tint previews as the rest of the catalog. Focus a tile to read its complete name/ID.
The desktop layout anchors panels to the actual window edges, uses spare space
for the viewport and derives control density from logical window size. Pointer
conversion uses the drawable separately, including doubled-density Retina math.
Actual macOS acceptance remains deferred for this phase.

More geometry and props remain in contextual sheets. Detailed controls follow the selection. Ramp Offset is
relative to the carrier room floor; Rise is signed toward the footprint's longer
axis, shown by UP +/-X/Z. Width/Depth choose the run axis (ties select X). Turn
uses quarter turns and preserves the two endpoint heights; square quarter turns
are refused because the format cannot express their changed axis. Recess Offset
is signed; negative lowers the basin and positive raises a platform. A region
has no independent thickness. Water remains an independent preserved collection.

Select a room, then `Shape >` to choose a snapped world-space rectangular
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

Construction/Safety is available directly in Simple and Advanced modes. It shares
the offline wall-plane/thickness classification, rectangular endpoint checks,
emitted opaque wall UV measurements and room-surface comparisons. Short gaps and
tails within 5 cm can offer **Weld / trim this endpoint** when one solid carrier
covers the full wall thickness. Existing overlapping L/T contacts are valid.
Near corners/footprint cracks up to 35 cm remain review-only beyond the weld
tolerance. Partial-thickness corners, competing carriers, intentional angles, openings and
different heights require review. Existing collinear union and covered-face
clipping remain authoritative; no arbitrary intersection is cut or flattened.

The review also samples player width/standing/crouch headroom at openings,
stairs, ramps and spawn, checks endpoint landings, and reports small room/floor/
ceiling seams. Solid carriers are clipped out of floor-gap findings through the
existing covered-face helper, including real opening slices. The loader retains slope/riser and closed-door validity gates;
Safety adds the authored door pose and local route evidence. Floor and wall
texture scale comes from the shared material table. Ceiling tile-frame and
measured square-corner phase breaks are review-only: a different orientation
can be intentional or invisible on symmetric artwork, and walls have no local
wrapping-frame correction. Generated trim retains its continuous run mapping.

**Preview fix** renders a private candidate; Close, Esc or **Back to finding**
restores the source view. Available corrections include exact simple duplicate
removal, identical-setting rectangular wall union, supported plane alignment,
structural ceiling clearance, small measured floor-prop mounting gaps and
restoration of a catalog prop's supported collision. Mounting and collision use
the existing tools; floating/embedded scenery, physics, fitted spans, arbitrary
wall mounts and ambiguous attachments are not inferred or moved.

Use **Choose for group** on each desired correction, then **Preview group**.
Conflicting edits or changed shared carriers refuse the whole group. **Apply
preview** commits compatible choices in one undo step and starts reanalysis.
Locked or hidden objects, protected placement intent and every intervening
source change block acceptance. Lock changes invalidate the cached review, so
unlocking can immediately run a fresh scan. Deletions address original records,
so array shifts cannot redirect another correction. Individual preview remains available.
**Cancel analysis** (or closing the review) cooperatively stops background work;
cancelled partial results never become findings or fixes. Full scans remain
deterministic and bounded; only the first 16 eligible plane-alignment proposals
receive planner post-checks per scan. No resumable/dirty-region cache is used.

The checks are not an exhaustive no-clip, watertightness, reachability or rendered
z-fighting proof. Edge sampling can miss narrow routes; motion, alpha pixels,
water, ladders and every camera/depth situation are not simulated. Oversized
scenes, missing models and exhausted budgets produce explicit incomplete-review
findings. Play-test jumps, shafts and pools. Source and manifest formats remain
unchanged; geometry revision 3 requires recompiling prior prepared output for
full prop orientation and the preceding ramp correction. See [stage 4 evidence and limits](reports/release-0.12.0.md#editor-0120-task4).

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
and [the native stage 6 evidence](reports/release-0.12.0.md#editor-0120-task6).

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

Project > Explore menu switches between **All maps** and **Entry only**. The
project-level setting saves scene edits first and publishes metadata atomically.
Entry only opens the declared entry map from Explore while retaining every
internal destination for transitions, direct diagnostic selection and import
conflict checks. Map settings, Allowed transitions, Map crosshair and Explore menu
have separate controls; the selected-level image remains in its dedicated sheet.

For explicit CPU latency checks, run `editor-profile PATH_TO_ISOLATED_PROJECT`.
It reports document operations and performs reversible in-memory transform/history
work. Native `--verify-script` recipes retain per-action handler and idle-wait
milliseconds in `timings.jsonl`. Neither is an FPS benchmark or a CI map bake.
`editor-profile --responsiveness PATH_TO_ISOLATED_PROJECT` additionally measures
cold/warm picker and thumbnail operations, safety scans, Inspect/Environment CPU
preparation and exact transform/history restoration against the real project.
It checks picker results against the uncached filter across themes, types and
queries. It opens no window, saves no source and performs no map bake; preview
CPU time does not include GPU upload or input-to-present latency.

For native performance evidence, `places-editor PROJECT --performance-test`
requests Immediate presentation and removes the normal 16 ms loop delay. It
refuses a surface that cannot select Immediate; it does not alter saved player
settings or ordinary editor pacing. Set `PLACES_LOAD_TRACE` to a fresh JSONL
path for frame/pump intervals, successful presentation flags, actual mode,
physical drawable, focus and the SDL-reported display refresh rate. The same
opt-in logs split device/driver bootstrap and pipeline creation from project
preparation. Run outside compilation/capture work. Inspect steady successful
frames and verify display pacing before drawing FPS conclusions;
request-to-ready and action-handler milliseconds are latency measurements.

### 0.13.0 selection, structure and bake review

Model/entity, fixture and material pickers show committed previews in an eight-tile
grid. Type to search, Backspace or Cmd/Ctrl A to clear, cycle the theme button,
scroll or use Left/Right to page, and use Tab/Enter to select. Missing artwork is
labelled and does not disable a valid asset. Only visible artwork is decoded,
one tile per frame; at most 24 tiles are retained. Model previews use the shipped
GLB's bind pose, actual textures and tint; they do not continuously animate or
render the entire catalog.

Project → Bake status checks **saved** levels in a background worker through the
installed compiler. Valid results reuse their output, metadata refresh reuses
prepared lighting, and stale results require a rebake. Integrity failure or
missing output remains invalid/unknown with its cause. Both the project export
and individual outputs are considered, so an old individual file cannot hide a
valid project member. The panel pages through all levels and gives a cause per row. Select an individual row
to build that level. Cancellation retains the last published package. Entry-byte
breakdowns are after ZIP decoding and before internal codecs; installed cache
and runtime RAM are separate and are not inferred from package size.

Drag the selected prop, light or isolated room's colored viewport axis. Toolbar
snapping applies, Escape/focus loss cancels, release commits one undo step, and
numeric controls remain. Isolated-room dragging carries supported contained
records; shared/stacked boundaries and coordinate-bearing global terrain/scripts
receive a refusal requiring explicit numeric/group review. It does not guess
which neighboring authored room should move.

For stairs and floor clearance, see the structural backing section of the map
authoring guide. Connect two floors previews the shaft cut and landings before
applying ordinary editable records. Cancel leaves source unchanged.


## Simple and Advanced authoring

The editor starts in **Simple**. `Mode` switches instantly to the existing
Advanced inspector and back; both operate on the same document, selection,
source buffers and undo history. Mode switching never converts level data.
Horizontal footprints are labelled Room1, Room2, etc.; Floor denotes a
vertical storey. Physical walking areas use Surface in controls and prompts. Display labels do not rename stable object or level references.

`New Room` opens a six-field form for position, size, floor height and ceiling
height. Its default footprint starts east of the selected room on the same
storey. Preview does not modify the document. Apply creates one history step;
Cancel restores the source view. A room supplies floor and ceiling; walls,
openings, doors and stairs are explicit geometry choices. `Move / exact size`,
`Choose model`, `Rooms / stairs` and `Events / scripts` expose focused workflows.
The existing actual-asset thumbnail browser, themes and numerical controls
remain available in both modes.

Select a room and drag a yellow edge handle to move that edge while keeping the
opposite edge fixed. The toolbar controls snapping; live dimensions show metres.
Release commits one undo step; Escape or loss of window focus cancels. Shape also
provides West/East/North/South numerical displacement fields. Connected outer
edges can expand; moving a shared boundary or overlapping another room is refused.
Perimeter wall slabs and their doors, wall fixtures and surface decals move in
one transaction. Side-wall openings keep their world positions. Unsupported
shrinks, cut openings, spanning walls and uncertain elevated fixed-prop attachments
are refused for explicit review. Ordinary content remains in world coordinates;
this is not a freeform wall topology solver. Axis handles provide translation
alongside precise controls for other supported scene objects.

## Event templates and behavior blocks

`Scripts > Event templates` offers start, interaction and volume entry/exit
handlers. Choose an NPC, item, target or room location and the options required
by the action, then Preview or Create. Supported actions include a message,
door/sound control, investigation, fetch/delivery, following, environment use,
and keyed AI cancellation/pause. Approach/contact coordinates are real world
positions: check the model, floor, reach and route in Play. Templates validate
references and shared-runtime limits; they cannot guarantee reachability.
Every creation writes a uniquely named editable module. It never overwrites an
existing source. `Blank module` starts custom code. Check reports the module and
Luau error location. Save, undo and source editing use the existing script editor.

Select a placed NPC and choose `NPC behaviors`. `Enable AI` fits a navigation
body only for a new unconfigured actor; it does not replace an existing setup.
Drag or click blocks into an ordered routine. Drag sibling rows to reorder.
Actions include movement, wait, investigation/search, fetch/delivery, environment
interaction, follow/escort and patrol. Hearing rules accept Any or selected event
categories and a radius. Sight rules filter player, entities, instance IDs or
object types and expose range/FOV; shared line-of-sight checks always apply.
Perception responses can investigate, run an ordered routine, or send a bounded
alert. Receive alerts is explicit. Routines stop on failure; task priorities
select the active task. Explicit loops stay cancellable. All execution uses the
shared AI runtime, physical item reservations/contact and existing navigation,
perception, recursion and per-frame budgets. The form refuses malformed plans,
missing references, empty routines and over-budget values.

`Apply` retains visual ownership. `New Luau copy` transfers execution to a new
module generated from the same typed plan. Editing that module cannot silently
rewrite the visual plan. Ordinary Apply is refused while Luau owns execution;
`Use visual` is the explicit choice to unlink the generated module while keeping
its file and edits. An externally shared event-bound module is not silently
unlinked. Pending visual drafts survive closing the panel and mode switches;
a scene revision mismatch requires explicit Reload before applying. Save/close
pending form edits by applying them, or explicitly Reload to discard them.

`Inspect last Play` displays the real player's latest AI/navigation snapshot,
including active/paused task keys, phase/step and last result. Refresh is explicit;
this is a snapshot of the last editor Play session, not a simulation in the form.
Stop Play keeps the snapshot available. Run the authored behavior to establish
physical reachability and inspect failure or cancellation in context.

Visual ownership metadata is stored beside the NPC as `editor_behavior` and
preserves the ordinary serialized AI component and Luau contracts. This phase
adds no package or source-format compatibility layer. Prepared data contracts
remain unchanged. The broad compiler source fingerprint includes renderer/debug
files, so this phase changes the compiler freshness stamp even though it does
not require a package-format migration.

## Light flicker

In either mode, select a ceiling or wall fixture and choose **Light flicker**
(also available in **Configure**). The effect button switches between two
independent forms; Apply changes only the displayed effect. Closing keeps pending
fields. Reload explicitly discards them. Applied changes use ordinary undo/redo,
Save, project export/import and Build + Play.

**Actual illumination** controls the selected fixture, or every fixture already
assigned to its shared circuit. Set a circuit in **Configure > Components > Light
controller > Shared circuit** first. A fixture on a circuit cannot be presented
as independently switchable: clear its circuit before choosing Individual.
The form enables Switchable for its members. All circuit members must share their
authored Casts light state. This form supports fixture-only circuits, up to eight
fixtures, within the engine's overall prepared-source limits.

Brief dropouts, Stutter burst and Slow outage presets expose steady seconds,
dark seconds, burst count, gap seconds, start delay and Run on load. When on,
the light uses its existing **Configure > Power**; when off, it is zero. These
controls do not provide an analogue room-light dimmer. **Full lighting** selects
the engine's existing prepared illumination layers, including the switched bounce
and live receiver contribution. The form holds authored light/face state in
Static unless the circuit already opts into prepared Static switching. Lightmaps
Off cannot animate baked vertex illumination. Preview the resulting lighting in
Build + Play, with the intended lighting mode.

**Glow only** animates the face/material emission with frequency, drop depth and
phase, using the existing `animated_emissions` flicker effect. It does not change
room illumination. Its asset ID explicitly applies to **all matching fixtures or
material surfaces in the level**, including other instances of the selected
fixture. Depth 0.6 can lower the face glow to 40% of its authored value. Off removes
that asset's animation entry. Applying Flicker replaces an existing Pulse entry
for that same asset; other assets and actual-illumination controllers are retained.

Actual controls create a readable linked Luau module using `api.light`; there is
no separate lighting runtime. **Edit generated Luau / handlers** opens its source.
The module exports `pause` (freeze phase/state), `resume` (unpause and unmute),
`mute` (hold off), `steady` (hold on), and `reset` (restore authored start settings).
Use the ordinary Scripts > Link handler picker to connect these to a prop, door
or trigger event without writing code. Run on load off starts paused; authored
Casts light off starts muted. Game pause stops simulation time, and a world reset
recreates the module. Execution has no wait loop and requests at most one light
state change per update. Direct unrelated `set_light` actions can be superseded
at the next flicker edge; use the module's mute/resume handlers for persistent
control.

Regeneration always writes a new source file. Ordinary Apply refuses to replace
a user-edited controller; **Replace link / keep source** explicitly transfers
ownership while preserving the old file and edit buffer. A module linked from
another event or the manifest cannot be silently unlinked: remove those links
before changing its generated preset, or edit its Luau directly. Disabling a
controller retains its settings/source and the existing Switchable/circuit flags.
Disable it before deleting a member, changing circuit membership/stable IDs or
changing Casts light; then reapply with the new arrangement. Editor metadata
`editor_flicker` records settings, members and source ownership without renaming
instance IDs or changing the package format. Blank/custom scripts remain available.


## Pillars and automatic trim (0.13.1)

Open **Geometry** (Simple mode: **Rooms / stairs**) and choose **Rectangular
pillar** or **Circular pillar**. The first sheet edits useful dimensions and
position. Rectangles use a minimum X/Z corner; circles use their centre and one
radius. Blank base Y follows the floor and blank height follows the ceiling.
Use **Trim >** for the wall material, trim placement and trim settings. Click
placement to cycle **none / bottom / top / both**. Defaults are 9 cm high and
18 mm thick with the installed white baseboard material.

After Apply, **Pillar >** reopens the selected shape's sheet. **Material** opens
the existing visual catalog browser for its wall finish. Pose/drag, Copy,
Delete, Undo/Redo and Save use the ordinary document paths. Circular transforms
expose one diameter; rectangular footprint rotations use quarter turns.
Simple mode omits record-navigation/delete controls from the pillar sheet;
Advanced retains the existing detailed authoring controls.

Select a wall and choose **Auto trim >** (also available from Geometry). Bottom
is the default. **Choose walls >** opens a paged checklist; the settings show
the chosen wall numbers and accept direct number entry too. Preview leaves
source/history unchanged; Apply commits the whole wall set in one undo step.
Repeated Apply and later geometry changes regenerate the saved attachments,
including walls moved/stretched by the existing room-edge tool. New walls follow
the local ceiling by default. Manual baseboard runs remain in the same system.
When an attached band coincides with a saved manual run, the manual run keeps
visible-face ownership; coincident fronts and caps are drawn once.

Inspect a preview from several angles. Use **Preview: Lighting** before opening
the sheet when checking illumination, then check the compiled game as well.
Ceiling bands include their visible underside and omit the cap coplanar with a
flat ceiling. Pillar/curve bands follow the primitive's actual segmented polygon;
straight walls and trim lose buried fragments at their junctions. Texture distance
continues around the perimeter instead of restarting at every segment.

Floor and ceiling lightmap charts also exclude the footprints hidden inside
walls, pillars and trim. This fixes a thin dark line that could look like a gap
at a bare wall-floor contact: hidden occluded samples no longer interpolate onto
the exposed edge. The same correction applies with trim present; light values,
shadow casting and the authored contact positions are retained.
Clipped triangular charts select the owning air side at all three edges with
rounding-scale separation, including their diagonal edge. A touching opaque wall
continues to block light; this does not add a geometric shadow bias.

Floor bands require a continuous flat floor contact. Wall/curve ceiling bands
require a flat closed ceiling; shorter pillar capitals can use an explicit post
height. Steps, sloping/open roof fits, incomplete support and excessive miters
receive a named error before Apply. These tools preserve authored geometry and
do not straighten intentional distortion. Trim remains decorative; the existing
pillar/wall solids provide collision and baked occlusion.

The existing Frame command uses bounding-box distance and can place its camera
outside a nearby outer wall. Zoom/orbit inside before judging a close contact;
that general camera behavior is outside this trim phase.

The small native validation source is
[`editor_trim_contacts.json`](../tests/fixtures/levels/editor_trim_contacts.json).
It includes bare and trimmed wall-floor contacts, both pillar shapes, all four
band choices, a straight wall entering a circular pillar, and an angled arc
junction. Production maps are not inputs to ordinary tests.
