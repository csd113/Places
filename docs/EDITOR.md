# Basic editor (0.12.0 task3)

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
the scene list independently. The shared renderer cuts away ceilings and uses
neutral preview light; Play shows authored ceilings, lighting and dynamic behavior.

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

Advanced geometry, custom importers, rich light/gameplay/trigger/fog/door tools,
diagnostics/auto-fixes, Luau, project bundles, menu scenes and final distribution
signoff remain later work. Other scene collections are preserved; controls edit
floors, walls and passive built-in props. Native Windows/Linux acceptance remains
outstanding.
