# Editor geometry and safety (0.12.0 task4)

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
| Numeric fields / adjacent minus and plus | Set position, dimensions, yaw, uniform prop scale |
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
unchanged; geometry revision 2 requires recompiling prior prepared output after
the descending-ramp surface correction. See [stage 4 evidence and limits](reports/editor-0.12.0-task4.md).

Custom importers, rich light/gameplay/trigger/fog/door tools, Luau, project bundles,
menu scenes and final distribution signoff remain later work. Other source
collections are preserved. Native Windows/Linux acceptance remains outstanding.
