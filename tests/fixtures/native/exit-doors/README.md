# Explicit native recipes

These are the saved action, editor and settings inputs for the bounded exit-door
native acceptance run. Use the approved private fixture or set it up through
the editor before a new run. The old Python level generator is historical.
Do not substitute a shipped map or recreate the geometry with a script.

Build the current source for the selected platform, run the read-only art gates,
then export/validate/verify the private project as in the dated Mac report.
For the player, copy `exit-doors.placesmap` into the chosen run directory's
`levels/` subdirectory. Launch the current `places` executable with these absolute
environment paths:

```
PLACES_ASSET_ROOT=<checkout>/assets
PLACES_STATE_ROOT=<run>
PLACES_CONFIG_ROOT=<run>
PLACES_CACHE_ROOT=<run>/cache
PLACES_LEVEL=exit_doors
PLACES_BENCH=1
PLACES_VERBOSE=1
PLACES_BENCH_ACTIONS=<run>/actions.json
PLACES_LOAD_TRACE=<run>/trace.jsonl
PLACES_STATE_LOG=<run>/player.csv
PLACES_BENCH_OUT=<run>/frames.csv
PLACES_BENCH_FRAMES=0
PLACES_BENCH_WARMUP=120
```

Run cases serially, with no competing native/build owner. The player scripts
quit themselves. `native-behavior` covers interaction/collision/locks/reset and
quality switches; `native-npc-flashlight` covers closed/open route clearance and
real flashlight geometry; `native-culling-final` covers open-leaf bounds.

For the editor, launch `places-editor <out>/editor-project --asset-root <assets>
--compiler <current places-map-compiler> --game <current places> --verify-script
<out>/native-editor-final/script.json --capture-dir <out>/native-editor-final`.
After it saves, launch the reopen and Scene-select scripts against that same
private editor project. Set each private script copy's `output` path to its own evidence directory.
Do not change the canonical map collection. Native input timing
is bounded evidence, not a substitute for inspecting the captures and snapshots.

Do not use the old preparation ZIP or Mac executables on Windows. Use the exact
published source commit, corrected committed GLBs/Blender sources and a native
Windows build. Windows backend/DPI and current computer state require their own
inspection. Mac results are not a Windows acceptance certificate. The room-light
casting limitation in [docs/EXIT_DOORS.md](https://github.com/csd113/Places/blob/29e12d9aa52b6c54f1d5d2d46cf8a524cf5862f7/docs/EXIT_DOORS.md) remains intentional.
