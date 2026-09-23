# Hardware benchmark suite

Everything here drives the release build on a real PocketCHIP over SSH. The
device does the measuring; these scripts only stage the payload, run the suite
and collect the CSV files, because the on-screen `-` performance overlay cannot
be read back over SSH.

## One-time cross-compile setup

The PocketCHIP runs Debian armhf. `sdl2-sys` needs a `pkg-config` that answers
for the *target*, so a small shim points it at the device's own `libSDL2`:

```sh
mkdir -p ~/.cache/liminal-armhf/lib
# copy /usr/lib/arm-linux-gnueabihf/libSDL2-2.0.so.0 from the device into that dir
ln -sf libSDL2-2.0.so.0 ~/.cache/liminal-armhf/lib/libSDL2.so
# ~/.cache/liminal-armhf/pkg-config is the shim (see run_bench.py)
PKG_CONFIG=$HOME/.cache/liminal-armhf/pkg-config \
PKG_CONFIG_ALLOW_CROSS=1 \
cargo zigbuild --release --target armv7-unknown-linux-gnueabihf
```

`run_bench.py` performs the cross-compile itself unless `--no-build` is passed.

## Attributing each optimisation

The submission switches exist so one release build can measure all four renderer
phases with the level build, the spatial partition, the draw order and the shader
held fixed. The `phase_ab` scene set uses them:

| Run | Culling | Indexing | Vertex layout | Is |
|---|---|---|---|---|
| `phase1` | off | off | 36 B floats | the pre-optimisation submission shape |
| `phase2` | **on** | off | 36 B floats | Phase 2: spatial culling |
| `phase3` | on | **on** | 36 B floats | Phase 3: indexed geometry |
| `phase4` | on | on | **24 B packed** | Phase 4: packed vertex (shipping) |
| `nocull` | off | on | 24 B packed | the culling delta alone |
| `noindex` | on | off | 24 B packed | the indexing delta alone |

Each row changes exactly one submission decision against its neighbour, so the
difference between two rows is that phase's contribution. macOS runs of this set
already give the hardware-independent part of the answer (submitted vertices,
buffer bytes, draw calls); the device run adds the frame times.

## Running a suite

```sh
python3 tools/bench/run_bench.py --phase phase2 --scenes cull_ab,away --repeat 3
```

That command

1. cross-compiles the release build for `armv7-unknown-linux-gnueabihf`;
2. uploads it, the shipped assets, the generated benchmark levels, the prop
   regression fixtures and the previous phase's binary to `/tmp/liminal-benchmark`;
3. generates a device-side shell script and runs every scene in one SSH session
   (connections are the slowest and flakiest part on PocketCHIP Wi-Fi);
4. downloads the per-frame CSVs and per-run logs into
   `tools/bench/results/<phase>/`;
5. prints the summary table and writes `summary.csv` / `summary.json`.

Remove everything the suite created with:

```sh
python3 tools/bench/run_bench.py --phase cleanup --cleanup
```

## Scene sets

| Set | What it measures |
|---|---|
| `chairs` | full prop-count curve, 0 → 1000 chairs, default camera |
| `chairs_core` | the 0/100/200/300/400/500 subset |
| `away` | 400 chairs with the camera pinned facing / away / side-on |
| `levels` | shipped `places_demo` plus the `prop_stress` and `prop_showcase` fixtures |
| `vsync` | renderer-only vs presentation-only vs `glFinish`-split runs, VSync on and off |
| `cull_ab` | the same build with culling on and off, plus the previous phase's binary |
| `cellsweep` | the spatial-grid-resolution trade-off |
| `levels_ab` | the demo and the prop fixtures under culling / no culling / previous phase |

`BIN=phase1` inside a scene's environment selects the pre-optimisation baseline
binary, so one suite can measure a phase against the exact build it must beat.

## Telemetry the game emits

All of it is gated behind `LIMINAL_BENCH=1`; a normal release run prints none of
it and allocates nothing per frame.

| Variable | Meaning |
|---|---|
| `LIMINAL_BENCH=1` | enables the harness (required for all of the below) |
| `LIMINAL_BENCH_OUT=file.csv` | per-frame rows: `frame,update_ms,render_ms,swap_ms,frame_ms,loop_ms,total_vertices,visible_vertices,culled_vertices,total_batches,visible_batches,draw_calls,vbo_bytes,index_bytes` |
| `LIMINAL_BENCH_WARMUP=n` | discard the first `n` frames |
| `LIMINAL_BENCH_FRAMES=n` | stop after `n` recorded frames and print the summary |
| `LIMINAL_CAMERA=yaw[,pitch]` | pin the camera for a repeatable shot |
| `LIMINAL_VSYNC=on\|off` | override the swap interval for VSync characterisation |
| `LIMINAL_BENCH_FINISH=1` | `glFinish` before the swap (splits renderer from presentation time) |
| `LIMINAL_BENCH_NORENDER=1` | skip scene/UI submission (presentation-only run) |
| `LIMINAL_BENCH_NOSWAP=1` | skip `SDL_GL_SwapWindow` (renderer-only run) |
| `LIMINAL_BENCH_NOCULL=1` | submit every batch (isolates what culling is worth) |
| `LIMINAL_BENCH_NOINDEX=1` | submit flat triangle lists (isolates what indexing is worth) |
| `LIMINAL_BENCH_EXACT_VERTEX=1` | upload the 36-byte exact vertex layout (isolates what packing is worth) |
| `LIMINAL_CELL_METRES=n` | force a uniform spatial grid instead of the adaptive one |
| `LIMINAL_LEVEL=<id>` | boot straight into a level |
| `LIMINAL_SPAWN=x[,y],z,yaw` | spawn override |
| `LIMINAL_CAPTURE=frame.png` | render one frame, write it, exit |

The run summary is printed as a single line:

```
BENCH_SUMMARY {"level":...,"frames":...,"swap_interval":...,"loop_median_ms":...,...}
```

`loop_ms` is begin-of-frame to begin-of-frame, i.e. the real presentation
cadence including the swap. `frame_ms` is begin-of-frame to end-of-swap. FPS
figures in the summary are always derived from `loop_ms`, never from a count of
renderer submissions.

## Analysis

```sh
python3 tools/bench/analyze.py tools/bench/results/phase2 \
    --csv tools/bench/results/phase2/summary.csv
```

`analyze.py` is also invoked automatically at the end of `run_bench.py`.

Notes on the measurements themselves live in `notes/`:

| Note | Contents |
|---|---|
| `renderer-change-validation.md` | how each change was validated, and what the pixel comparison actually measures |
| `level-build-cache.md` | what a level load costs, and a proposed build-cache key/invalidation design |
| `lightmap-bake-validation.md` | Batch 2 baked-lightmap measurements: cold/warm bake cost, Full/Low density and memory, runtime draw calls, vertex-memory cost and the lightmap-vs-vertex pixel A/B |

## Visual regression

Renderer changes must be pixel-identical when they only reorder or re-batch the
same geometry:

```sh
python3 tools/bench/visual_check.py \
    --baseline target/phase1/liminal-rust-macos \
    --current  target/release/liminal-rust
```

It runs a fixed list of levels and camera states through `LIMINAL_CAPTURE`,
decodes both sets of PNGs and reports the number of differing pixels, the
fraction of the image, and the worst channel delta per shot.

## Lightmap bake and lighting A/B

Batch 2's static lightmaps are measured with a dedicated driver: it runs the
one-frame capture path plus `LIMINAL_BENCH` telemetry over a fixed shot list,
parses the `[level]`/`[lighting]`/`[lightmaps]`/`[spatial]` developer lines and
writes `report.json` beside the PNGs and per-frame CSVs.

```sh
# cold bake + captures for the standard shot list
python3 tools/bench/lightmap_report.py --label full --cold

# the exact vertex-lit control run (same build, lightmaps forced off)
LIMINAL_NO_LIGHTMAPS=1 python3 tools/bench/lightmap_report.py --label vertex

# a specific profile, run from a package directory holding its settings.json
python3 tools/bench/lightmap_report.py --label low \
    --run-dir $PWD/target/agent-work/benchmarks/run-low
```

Fixtures under `tests/fixtures/levels/` are staged into a package directory under
`target/agent-work/` (via `LIMINAL_ASSET_ROOT`), never copied into the
repository's own `levels/`. `tools/bench/notes/lightmap-bake-validation.md` holds
the measurements taken with it.

## Level generation

`gen_levels.py` writes the chair-stress levels as ordinary community-format
`LevelDef` JSON, so nothing about the benchmark bypasses the normal load path.
The spawn faces the whole chair grid, which makes `LIMINAL_CAMERA=180` the
"facing" view and `LIMINAL_CAMERA=0` the camera-away view.
