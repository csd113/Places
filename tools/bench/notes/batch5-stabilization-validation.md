# Batch 5 validation: compiled build, cleanup, QA and the performance baseline

How the Batch 5 stabilization pass was verified. Everything here was run on the
development machine (macOS, 960x544 drawable) with the shipped assets and
Places Demo; all scratch output lives under `target/agent-work/`.

## Compiled build and first run

`tests/test_compiled_build.py` runs the actual release executable, copied into a
scratch directory, from a working directory outside the repository. It asserts:

* a portable package (executable + `assets/`) boots from an unrelated working
  directory, resolves its payload from its own path and writes its state next to
  the payload, not in the working directory;
* an empty first-run install creates `levels/`, `import/`, `cache/` and a default
  `settings.json`, and boots the embedded Places Demo with no asset tree at all;
* a restart loads the saved configuration (a hand-edited FOV/filtering survives);
* a malformed `settings.json` is preserved as `settings.json.invalid`, defaults
  are used and a clean file is written;
* malformed custom levels are skipped with the file name and reason;
* unknown materials, props, fixtures and glass degrade with diagnostics and the
  level still renders;
* Places Demo always loads by id;
* normal runs print no developer telemetry.

Run it with:

```sh
cargo build --release
python3 -m unittest tests.test_compiled_build -v
```

## Performance

Same machine, same level, assets and camera (`places_demo`, camera `74,0`,
120 frames after 20 warm-up, `LIMINAL_VSYNC=off`, `LIMINAL_BENCH_FINISH=1`,
7 repeats). `b4_baseline` is commit `60c86f7` (Batch 4) built in a worktree and
run with the same content:

| Run | `render_mean_ms` min / median | `loop_median_ms` | draws | binds | material changes |
| --- | --- | --- | --- | --- | --- |
| `b4_baseline` Full | 0.963 / 1.009 | 1.108 | 77 | 97 | 77 |
| `b5_full` (this build) | 0.958 / 1.024 | 1.111 | 77 | 97 | 77 |
| `b4_baseline` Low | 0.438 / 0.448 | 0.563 | 77 | 48 | 38 |
| `b5_low` (this build) | 0.442 / 0.450 | 0.531 | 77 | 48 | 38 |

Reading the numbers:

* **No material regression.** Full render min is 0.5 % faster and median 1.5 %
  slower than the Batch 4 baseline, both inside the run-to-run spread; Low is
  flat. Draw calls, texture binds and material changes are identical.
* **Vertex memory +88 vertices** (12 114 → 12 202, +0.7 %): the second pool
  notice board's bottom rail is now visible above the deck instead of buried
  below it. It is the only content change to the demo.
* The numbers were taken before the small startup fix below; the fix does not
  touch the frame path.

### Level build and cache

`LIMINAL_VERBOSE=1` on the same level:

| Path | Level build | Lightmap fill | Notes |
| --- | --- | --- | --- |
| cold (no cache) | 191.9 ms | 173.9 ms | bakes 1 page / 350 charts |
| warm (cache hit) | 9–18 ms | 0.0 ms | 2.4 ms probe bake, ~7 ms prop decode |

`LIMINAL_LEVEL=<the level already loaded>` no longer rebuilds it: the game used
to run the whole cold level build twice at startup in that case (once for the
default level, once for the request). A cold verbose run now prints exactly one
`[level]`/`[lightmaps]` build.

Memory at 960x544 Full, from the same telemetry: lightmap atlas 3 072 KiB
(1 048 576 page texels), prop textures 752 KiB for 14 models (2 724 triangles),
plus the Batch 4 post/reflection targets documented in
`batch4-post-reflection-validation.md`.

## Places Demo QA

A full capture playthrough (20 views: reception, workroom, stair hall top and
mid, pool entry/wide/basin/steps/deep corner, corridor entry/mid/end, final
doorway, unmade world and back, ceiling, floor-elevation overview) in Full and
Low, each image inspected at full size:

* no Z-fighting, geometry holes, exposed interiors, incorrect materials,
  floating props, broken shadow shapes, light leaks, lightmap seams, broken
  transparency, reflection artifacts or clipping was found;
* the intentional items (the unmade world's missing side walls beyond the final
  doorway, the red stair-hall lighting on the stained wallpaper, the wet deck's
  planar reflection, the backlit signs) were confirmed against the level file;
* **one fix**: the second pool notice board's bottom rail was authored at
  `y = -1.66`, entirely below the deck at `y = -1.5`, so the "four-rail frame"
  had three visible rails. It now sits at `y = -1.49`, offset in `z` so no
  architecture faces are coplanar; the shipped-demo surface audit
  (`the_shipped_demo_has_no_coincident_architecture_surfaces`,
  `places_demo_doorway_thresholds_are_owned_once`) still passes.

The capture poses themselves had a bug on the first pass: `LIMINAL_CAMERA` yaw
is absolute and replaces the spawn yaw, so corridor views aimed sideways and
four frames rendered only background. The committed `capture_views.sh` view
table now carries the correct absolute yaw for every walkthrough view.

## Error paths

Beyond the compiled-build cases above, the Rust suites cover the malformed
content paths, and Batch 5 added:

* a ZIP entry that lies about its decompressed size cannot expand past the
  per-entry cap (`test_zip_reads_are_capped_by_output_not_the_declared_size`);
* a GLB accessor with `byteStride: 0` and a large `count` is rejected
  (`rejects_zero_stride_accessors_that_claim_many_elements`) and an oversized
  count is refused before allocation
  (`rejects_a_count_larger_than_the_buffer_can_hold`);
* too many floor patches and too many per-wall openings are named loader
  rejections;
* a malformed settings file is preserved and recovered, reserved/duplicate
  bindings are repaired, and action labels are player-facing.

## Validation commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 -m unittest tests.test_compiled_build
```

The Batch 3/4 capture and device-suite scripts were retired in this pass; the
historical measurements remain in the other notes in this directory.
