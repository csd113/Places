# Benchmark and capture suite

Everything here drives the release binary on this development machine (no
device, no SSH) and writes its artifacts below `target/agent-work/`. Those
artifacts are generated locally and **never committed**: `target/` and the
result directories are ignored, so re-running a suite is the way to reproduce
a number, not a file in the repository.

The current workflow uses the following tools:

| tool | what it does |
| --- | --- |
| `bench_local.py` | repeats one benchmark configuration and prints min/median/max per field |
| `capture_views.sh` | renders the fixed validation view set, one PNG per view |
| `capture_baseline_views.sh` | renders the canonical pre-wgpu baseline view set in the High and Low profiles |
| `capture_expanded_views.sh` | renders the Stage 10 supplementary parity view set (geometry, materials, lightmaps, reflections, props, decals, fog) for the same two-backend comparison |
| `baseline_asset_root.sh` | builds a scratch asset root matching the committed revision, for baseline captures while the working tree's assets are mid-edit |
| `compare_baseline.py` | checks a canonical capture directory against the frozen PNGs |
| `compare_captures.py` | reports per-view pixel-difference statistics between two capture directories |
| `visual_check.py` | decodes two capture sets and reports per-shot pixel differences |
| `lightmap_report.py` | runs the bake/lighting shot list and writes a `report.json` |
| `check_holes.py` | counts near-black pixels in captures, to catch holes in a level shell |

## Local benchmark

`bench_local.py` runs the release binary directly and repeats one configuration
`--repeat` times. It prints the minimum, median and maximum of every headline
field (the minimum is the number least contaminated by unrelated system work)
and writes the same numbers to `target/agent-work/bench/<label>.json`.

```sh
# the Full profile, five repeats
python3 tools/bench/bench_local.py --label current_full --repeat 5

# the Low profile on the same build
python3 tools/bench/bench_local.py --label current_low --repeat 5 --quality low

# a baseline checkout, for a before/after pair
python3 tools/bench/bench_local.py --label baseline --repeat 5 \
    --binary target/agent-work/baseline/target/release/places
```

Flags:

| flag | effect |
| --- | --- |
| `--label NAME` | output name; the JSON lands at `target/agent-work/bench/NAME.json` |
| `--repeat N` | how many whole runs to take the min/median/max over (default 3) |
| `--quality full\|low` | draw this run at the named profile without editing `settings.json` |
| `--direct` | `PLACES_NO_OFFSCREEN=1`: draw straight into the default framebuffer |
| `--no-lightmaps` | `PLACES_NO_LIGHTMAPS=1`: force the vertex-lit path |
| `--binary PATH` | measure another executable (default `target/release/places`) |
| `--level ID`, `--camera yaw[,pitch]` | the fixed scene (default `places_demo`, `74,0`) |
| `--frames N`, `--warmup N` | recorded frames and discarded warm-up frames (default 120 / 20) |
| `--finish` | insert `glFinish` before the swap, splitting renderer from presentation time |
| `--noswap` | skip `SDL_GL_SwapWindow`, so a run cannot block on a display that has gone to sleep |
| `--timeout S` | seconds before one run is treated as hung (default 300) |

Every run holds the level, assets, camera, frame count and swap interval fixed,
so only the flag you changed differs between two labels.

## Capture views

`capture_views.sh` renders the fixed validation view set: the surface-response
and post-processing shots (windows, panels, deck, sign, linoleum, pause menu)
plus a walkthrough of the demo's route from reception through the Home wing. Each
view nails its spawn and camera, so the same command produces the same image on
any machine and the before/after and Full/Low sets are directly comparable.

```sh
sh tools/bench/capture_views.sh                            # Full profile
PLACES_QUALITY=low sh tools/bench/capture_views.sh        # profile suffix _low
PLACES_NO_OFFSCREEN=1 sh tools/bench/capture_views.sh     # _direct
PLACES_NO_BLOOM=1 sh tools/bench/capture_views.sh         # _nobloom
PLACES_NO_REFLECTIONS=1 sh tools/bench/capture_views.sh   # _norefl
```

Files are written as `view_<name><suffix>.png` to
`target/agent-work/captures/` by default. `PLACES_CAPTURE_DIR` overrides the
output directory, and `PLACES_BIN` points the same view table at another build,
which is how a before/after pair is captured without overwriting the reference:

```sh
PLACES_BIN=target/agent-work/baseline/target/release/places \
    PLACES_CAPTURE_DIR=target/agent-work/captures_baseline \
    sh tools/bench/capture_views.sh
```

The suffix accumulates in the order low / direct / nobloom / norefl, so a
comparison run never overwrites the reference capture.

## Canonical renderer baseline

`capture_baseline_views.sh` renders the smaller, curated view set that is the
permanent pre-wgpu reference for the renderer migration, in both quality
profiles at once. It owns its view table, a pinned `settings.json` under
`target/renderer-baseline-state/` (a 640x360 logical window, which is a
1280x720 drawable on a 2x display) and writes `<name>.png` plus `manifest.txt`
per profile:

```sh
sh tools/bench/capture_baseline_views.sh                        # -> docs/renderer-baseline/{high,low}
PLACES_QUALITY=low sh tools/bench/capture_baseline_views.sh    # one profile only
PLACES_BIN=target/release/places-wgpu \
    PLACES_CAPTURE_DIR=target/agent-work/wgpu-baseline \
    sh tools/bench/capture_baseline_views.sh                    # a future renderer
```

The committed reference and its camera/settings manifest are documented in
`docs/renderer-baseline/BASELINE.md`; that document is the authority on what
each view exercises. `PLACES_QUALITY=full` and `PLACES_QUALITY=low` select one
profile, and any other value is rejected. Delete the state root before a run to
force a cold lightmap bake rather than reusing its cache.

The working tree's assets can be mid-edit while renderer work continues, in
which case a capture legitimately differs from the committed reference. To
compare against the renderer rather than the assets, build a scratch asset tree
from the committed revision and point the binary at it:

```sh
PLACES_BASELINE_ASSET_ROOT=target/agent-work/stage8/asset-root \
    sh tools/bench/baseline_asset_root.sh
PLACES_ASSET_ROOT="$PWD/target/agent-work/stage8/asset-root" \
    sh tools/bench/capture_baseline_views.sh
python3 tools/bench/compare_baseline.py <capture-dir>   # byte equality
```

`compare_captures.py` is the same comparison for two *different* renderers,
where byte equality is not the goal: it reports each view's mean absolute
channel difference, the share of pixels differing by more than a tolerance, and
the maximum channel difference, so a migration's known gaps (unported geometry,
fog, post-processing) can be quantified instead of hand-waved.

```sh
python3 tools/bench/compare_captures.py \
    target/agent-work/stage8/wgpu-baseline-assets \
    target/agent-work/stage8/opengl-vertexlit \
    --label wgpu-stage8 opengl-vertexlit
```

## Visual regression

`visual_check.py` drives the one-frame capture path from two builds over a fixed
shot list, decodes both sets of PNGs and reports the number of differing pixels,
the fraction of the image and the worst channel delta per shot. It fails on the
largest *connected* group of significant pixels exceeding `--max-component`
(default 256), which separates a one-row float-rounding sliver from dropped or
extra geometry.

```sh
python3 tools/bench/visual_check.py \
    --baseline "$PWD/target/agent-work/baseline/places" \
    --current  "$PWD/target/release/places"
```

Pass **absolute** binary paths: each capture runs with the staged package as its
working directory, so a repository-relative path does not resolve. `--out`
selects where the captures and the staged package root live; the run
symlinks the shipped `assets/` and the `tests/fixtures/levels/` fixtures into it
and points `PLACES_ASSET_ROOT` there, so the demo and the regression fixtures
resolve without copying anything into the repository's own `levels/`.
`--tolerance`, `--max-component` and `--strict` adjust the gate.

## Lightmap bake and lighting A/B

`lightmap_report.py` runs the one-frame capture path plus `PLACES_BENCH`
telemetry over the bake/lighting shot list, parses the
`[level]`/`[lighting]`/`[lightmaps]`/`[spatial]` developer lines and writes
`report.json` beside the PNGs and per-frame CSVs.

The parsed `[level]`/`[lighting]`/`[lightmaps]`/`[spatial]` lines are only
emitted by a verbose run, so pass `--env PLACES_VERBOSE=1` to populate the
report's metric fields; without it the report still captures every shot but its
numbers are empty.

```sh
# cold bake + captures for the standard shot list
python3 tools/bench/lightmap_report.py --label full --cold --env PLACES_VERBOSE=1

# the exact vertex-lit control run (same build, lightmaps forced off)
PLACES_NO_LIGHTMAPS=1 python3 tools/bench/lightmap_report.py --label vertex

# a specific profile, run from a package directory holding its settings.json
python3 tools/bench/lightmap_report.py --label low \
    --run-dir $PWD/target/agent-work/benchmarks/run-low
```

Fixtures under `tests/fixtures/levels/` are staged into a package directory under
`target/agent-work/` (via `PLACES_ASSET_ROOT`), never copied into the
repository's own `levels/`. `--cold` deletes that run directory's
`cache/lightmaps/` (the runtime lightmap cache below the state root) first, so
the next run bakes for real.

## Checking captures for holes

`check_holes.py` counts near-black pixels in captured PNGs. A shelled room never
renders the clear colour, so a capture from inside it must have almost no fully
black pixels; a block of them is a missing wall, floor or ceiling.

```sh
python3 tools/bench/check_holes.py target/agent-work/captures/view_*.png
python3 tools/bench/check_holes.py --threshold 8 --step 2 target/agent-work/captures/
```

It exits non-zero when any capture exceeds `--max-fraction` (default 0.05), so
it can gate a capture matrix.

## Telemetry the game emits

All of it is gated behind `PLACES_BENCH=1`; a normal release run prints none of
it and allocates nothing per frame.

| Variable | Meaning |
|---|---|
| `PLACES_BENCH=1` | enables the harness (required for all of the below) |
| `PLACES_BENCH_OUT=file.csv` | per-frame rows: `frame,update_ms,render_ms,swap_ms,frame_ms,loop_ms,total_vertices,visible_vertices,culled_vertices,total_batches,visible_batches,draw_calls,vbo_bytes,index_bytes,texture_binds,material_changes,reflection_passes` |
| `PLACES_BENCH_WARMUP=n` | discard the first `n` frames |
| `PLACES_BENCH_FRAMES=n` | stop after `n` recorded frames and print the summary |
| `PLACES_CAMERA=yaw[,pitch]` | pin the camera for a repeatable shot |
| `PLACES_VSYNC=on\|off` | override the swap interval for VSync characterisation |
| `PLACES_BENCH_FINISH=1` | `glFinish` before the swap (splits renderer from presentation time) |
| `PLACES_BENCH_NORENDER=1` | skip scene/UI submission (presentation-only run) |
| `PLACES_BENCH_NOSWAP=1` | skip `SDL_GL_SwapWindow` (renderer-only run) |
| `PLACES_BENCH_NOCULL=1` | submit every batch (isolates what culling is worth) |
| `PLACES_BENCH_NOINDEX=1` | submit flat triangle lists (isolates what indexing is worth) |
| `PLACES_BENCH_EXACT_VERTEX=1` | upload the 36-byte exact vertex layout (isolates what packing is worth) |
| `PLACES_CELL_METRES=n` | force a uniform spatial grid instead of the adaptive one |
| `PLACES_LEVEL=<id>` | boot straight into a level |
| `PLACES_SPAWN=x,z[,yaw]` or `x,y,z[,yaw]` | spawn override; the 3-number form drops the player onto the local floor |
| `PLACES_CAPTURE=frame.png` | render one frame, write it, exit |
| `PLACES_CAPTURE_FRAME=n` | which frame to capture (default 1), so a moving object can be captured mid-animation |
| `PLACES_NO_LIGHTMAPS=1` | force the vertex-lit path for a lightmap A/B capture |
| `PLACES_DUMP_LIGHTMAPS=1` | write baked atlas pages as PNGs under `target/agent-work/atlases/` (a fresh bake only — delete `cache/lightmaps/` first, since a cache hit writes nothing) |
| `PLACES_QUALITY=full\|low` | draw this run at the named profile without editing `settings.json` |
| `PLACES_BENCH_QUALITY_CYCLE=<frame>:<profile>[,<frame>:<profile>...]` | scripted live quality switch through the normal settings path (benchmark only) |
| `PLACES_BENCH_WINDOW_CYCLE=<frame>:resize:<w>x<h>\|<frame>:minimize\|<frame>:restore[,...]` | scripted live window events through the real SDL window: resize, minimize, restore (benchmark only; Stage 10 lifecycle matrix) |
| `PLACES_NO_OFFSCREEN=1` | OpenGL-only: skip the offscreen scene target and draw into the default framebuffer |
| `PLACES_NO_BLOOM=1` | drop the emissive pass and the blur, keeping the resolve stage |
| `PLACES_NO_REFLECTIONS=1` | report every material as reflection-free (no planar pass, no probe bake, no reflection binds) |
| `PLACES_PAUSE=1` | open the pause menu on the first frame, so the pause UI can be captured without a keyboard |
| `PLACES_VERBOSE=1` | print the startup/level-build telemetry a capture run usually stays silent about |

The demo's emission animations advance on the simulation's own delta, so a
capture at `PLACES_CAPTURE_FRAME=1` is always the authored brightness; later
frames show whatever the elapsed clock reached, which is what a flicker capture
wants. The summary reports `reflection_passes` (scene submissions the frame
spent on the active planar plane) alongside the timing and counter fields.

The run summary is printed as a single line:

```
BENCH_SUMMARY {"level":...,"frames":...,"swap_interval":...,"loop_median_ms":...,...}
```

`loop_ms` is begin-of-frame to begin-of-frame, i.e. the real presentation
cadence including the swap. `frame_ms` is begin-of-frame to end-of-swap. FPS
figures in the summary are always derived from `loop_ms`, never from a count of
renderer submissions.
