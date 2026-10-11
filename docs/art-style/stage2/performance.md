# Stage 2 costs — 2026-10-08 UTC

Same Mac 14,9 / Apple M 2 Pro, 12 CPU /19 GPU cores, 16 GB unified memory,
macOS 27.0.1 native Metal as [Stage 1](../performance.md). Window 640×360 logical,
1280×720 drawable, VSync requested Off and applied Immediate. Fixed hero cameras,
source/assets/lights/environment remain unchanged. No concurrent build/test/bake
or other task-owned native worker runs during these six sequential samples.
Ordinary desktop applications still run; this is one host, not an isolated GPU
laboratory. Measurements use the bounded method below.

## Submitted runtime samples

120 warmup +360 recorded frames per quality/view; **360/360 nonzero scene-draw
frames in every sample**. Timings measure CPU work/submission and event-loop
pacing, not GPU execution/display refresh. The captured screenshot readback is
absent from these measured samples.

| Preset / view | Before → after median render CPU ms | After frame CPU median /p95 ms | After event-loop median /p95 ms | Accounted draws / range-or-object vertices | Before → after peak RSS MiB |
| --- | --- | --- | --- | --- | --- |
| High / entities | 7.130 → 5.893 | 6.023 / 13.811 | 6.314 / 14.292 | 17 / 10668 | 431.5 → 429.6 |
| High / hall | 7.140 → 2.825 | 2.891 / 12.054 | 3.195 / 12.390 | 8 / 212 | 432.4 → 430.1 |
| High / room | 7.098 → 4.069 | 4.139 / 14.073 | 4.373 / 14.653 | 31 / 21008 | 433.3 → 430.0 |
| High / window | 6.925 → 2.671 | 2.758 / 13.341 | 3.135 / 13.881 | 18 / 9960 | 431.3 → 432.3 |
| Low / room | 7.354 → 4.096 | 4.178 / 13.331 | 4.412 / 13.581 | 31 / 21104 | 175.9 → 176.3 |
| Medium / room | 7.289 → 4.162 | 4.207 / 13.773 | 4.397 / 14.239 | 31 / 21008 | 365.9 → 372.2 |

Observed CPU timings are lower in this campaign; **no GPU speedup or minimum-FPS
claim follows**. Surface/scheduler pacing, warm driver caches and ordinary desktop
conditions can affect these short samples. No GPU timestamp/profiler, long thermal
run or cross-platform measurement is available. Budget/cap changes are not justified.

The final counter fix removes phantom object vertices for empty alpha classes.
Stage 1 and the first Stage 2 galleries counted the opaque chair again during an
empty translucent visit; adding correct cutout visits exposed another extra count.
Final High room/entities report 21,008/10,668 rather than 21,440/11,100. This is
accounting, not dropped geometry or reduced content. Per-object buffers may still
be counted once per *submitted* alpha class on a mixed-class mesh; counts are not
globally deduplicated triangles or GPU occlusion-visible vertices. Draws remain
31/8/18/17 for room/hall/window/entities. Scope excludes sky/reflections, emissive
duplicates, post and UI. Low flat-corner duplication increases its vertex storage,
without altering source topology, collision or asset triangle counts.

## Allocation and package costs

- World vertex stride 64→76 bytes (+18.75%); High/Medium aggregate benchmark
  VBO 1,063,424→1,262,816 bytes. Index storage 33,900 bytes is unchanged. The
  environment normal matrix adds 48 bytes to each model environment uniform.
  Low final VBO is 1,271,176 bytes/index storage 34,752 bytes after flat-corner
  expansion; source triangles and batching bounds are retained.
- Colour/data texture storage remains 4 bytes/texel. World cache receipts remain
  High 53,127,840 /Medium 17,476,256 /Low 7,514,784 bytes, including the dedicated
  committed white fallback. Props, numeric normals, sky and target resources are
  separate; this is not total GPU memory. High prop texture residency 5,264,704
  bytes includes fitted mips; source decoded prop inventory remains 3,856 KiB.
- Scene/emission/both blur buffers move 4→8 bytes/texel; presented image stays 4.
  The five colour-image allocations at High total 11,520,000→19,353,600 bytes
  (+7,833,600); Medium 5,644,800→7,603,200; Low 4,787,520→5,888,640. These are
  deterministic **colour allocations only**, excluding depth/driver alignment,
  sky/cache/reflections/CPU copies; Low's allocated blur images need not execute.
- Full 64 px cube mip-chain texels total 32,766:131,064→262,128 bytes. Medium 48 px
  chain totals 18,426:73,704→147,408 bytes. The same one selected hero probe,
  face size/roughness LOD and no planar target remain. No reflection-resolution
  increase or extra probe is introduced.
- Full/Medium atlas pages remain 2,1024² paired irradiance+moment layers,
  33,554,432 bytes, 5,347 charts /167,960 chart texels.495 irradiance slots
  (260 air-valid),16 static model resources and 5,300 source triangles remain.
- Hero package 3,873,651→4,521,326 bytes (+647,675,+16.72%).
  Member comparison: HDR cube blobs contribute
  +579,661 compressed bytes; flat fallback/scalar prop records +76,733;
  lightmap compressed data −8,740. Uncompressed lightmap storage is unchanged.
  Source/model/texture/catalog/concept bytes are unchanged.

Colour preparation performs linear box filtering and alpha weighting at upload;
CPU work is off the frame path. Shader evaluation removes the light soft clip
and unlit branch, adds inverse-transpose attributes and shared scalar sheen.
Native Metal validates the actual shader paths. Per-shader GPU execution is
unmeasured; CPU render samples are not a substitute for that cost.

## Forced compilation and reproducibility

Stage 1 forced compiler 4.872 s/process 4.89 s, peak 659.1 MiB. Stage 2 final isolated
forced compiler 7.149 s/process 7.44 s, peak 660.7 MiB;12-worker cap, all three normal
variants.
The slower overall run is concentrated in the first capture phase:
Off capture 127.18→2506.42 ms. This includes new native HDR pipeline/capture setup
and prefiltering, not solely the lighting solve. Medium preparation 745.52→732.43 ms;
Full 3388.98→3246.68 ms. These phase timings do not prove a solver speedup.
The first build overlapped library tests (11.44 s) and is excluded, preserved as
`hero-build-initial.*`. Two independent bakes reproduce the exact package SHA
and all six independent native PNGs match byte for byte.

Texture size/filter settings remain independent of lighting selection, with
trilinear 4×/8×/16× policies and device support fallback. Final High/Medium/Low
samples and native controls confirm applied state.
Single-sample silhouettes, alpha-test coverage mips, emission alpha/family sorting
and exposure are documented later-stage limits; no cost is hidden by disabling
assertions, quality, content, bloom or shadows.
