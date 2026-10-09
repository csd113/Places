# Stage 3 measured costs — 2026-10-08 UTC

Same Mac 14,9 / Apple M 2 Pro, 12 CPU /19 GPU cores, 16 GB unified memory,
native macOS Metal as [Stage 2](../stage2/performance.md). All native jobs,
bakes and measurements run sequentially; no task build or test overlaps a
measured bake or frame sample. Ordinary desktop applications remain running.
The original hero, lights, source art, display settings and six cameras are
unchanged. The separate control is an additive arrangement of that same hero.

## Normal forced compilation

Final compiler receipt, verbose phase log
and OS receipt measure the normal release compiler,
12 workers, all Off/Medium/Full variants, forced output. Dump instrumentation is
absent. Stage 2's normal forced run is reused rather than recompiling history.

| Measurement | Stage 2 | Stage 3 final hero |
| --- | --- | --- |
| Compiler / OS elapsed | 7.149 /7.44 s | 9.804 /10.08 s |
| OS maximum resident memory | 660.7 MiB | 646.7 MiB |
| Medium preparation and encoding | 732.43 ms | 1416.09 ms |
| Full preparation and encoding | 3246.68 ms | 5286.67 ms |
| Off reflection capture/setup | 2506.42 ms | 2405.08 ms |
| Medium /Full capture | prior receipt | 101.82 /100.65 ms |
| Dependency loading | prior receipt | 60.23 ms |
| Package | 4,521,326 bytes | 5,187,045 bytes (+14.72%) |

The preceding sealed v 3 run was 7.526 /7.81 s, with
194.75 ms first capture/setup rather than 2405.08 ms. The v 4 repeat before the cap hint repair was 9.837 /10.13 s with Full
preparation 5249.83 ms; the final Full preparation is 5286.67 ms. The cap repair
alters package room hints and fill, so package equality is not claimed. Most
of the end-to-end variation is
native setup/capture cost, not a lighting speedup. The final cost is higher than
Stage 2's normal run. Physical
endpoint samples and coverage integration do more useful work, and real cutouts
and Blend crossings add ray evaluation. No quality, source, bounce or atlas-page
budget is reduced to compensate. One run cannot establish a stable RSS gain.

| Normal transport phase | Medium ms | Full ms |
| --- | --- | --- |
| Receiver support and sample placement | 70.954 | 98.306 |
| Direct visibility and finite-source integration | 247.646 | 889.680 |
| Diffuse gather /GI | 593.342 | 3590.644 |
| Existing probe field | 7.557 | 9.135 |
| Diffuse filtering | 320.867 | 506.951 |
| Baseline fill | 20.860 | 34.610 |

The final run records scene construction 4.845 /5.623 ms,
199.32 ms hashing and 143.50 ms archive publication. Phase totals include bounded
worker overhead and need not sum exactly to compiler preparation. Direct time
includes visibility and accumulation; no unsupported standalone BVH-only timing
is claimed. Serialization, capture, hashing and publication remain separately
reported compiler phases. Controlled annex costs
10.264 s compiler time and 6,782,162 bytes; it is a larger diagnostic arrangement,
not a replacement benchmark for the original hero.

## Storage and useful work

Package member comparison preserves byte and compressed
member sizes. The page array is still two 1024² pages with paired float irradiance
and moments:33,554,432 resident bytes per Medium/Full variant. Uncompressed KTX 2
is 33,554,628 bytes per variant including headers. More detailed fields compress
less: the two compressed lightmap blobs total 3,543,709 bytes versus 3,014,500.
No texture/concept/model file is changed. Five thousand three hundred model
triangles, 16 prop batches, 495 probe slots /260 air-valid probes, 3194 navigation
cells /2 regions are retained. Architecture now includes real exposed rigid wall
tops and consistent roof-profile cuts:23 collision spans instead of 21, covering
the same authored solids rather than adding a gameplay barrier.

Full chart count 5347→5354 and receivers 167,960→203,798 (+21.34%). Positive spans
retain both endpoints;6205 one-sample axes become zero. Padding/bounds audit finds
zero overlapping reservations in both versions. Medium uses the same geometric
chart set with its existing density. Physical footprint scouts and bounded
coverage sampling increase direct work without changing source tap counts.

Finite-source tap iteration removes per-receiver temporary Vec allocations.
Transport images share decoded compiler allocations through Arc. A sparse Blend
node mask prunes opaque-only subtrees; no new cross-build cache or dependency
framework is introduced. The unchanged-package build exits 0 in 263.39 ms,
reports `rebuilt:false`, and preserves the normal package hash. It spends 37.29 ms
loading dependencies and 225.11 ms checking package integrity, with no transport
or native reflection recapture. Stage 6 owns the documented live material-dependency key gap.

## Submitted runtime samples

Four sequential final High samples,
parsed distributions:120 warmup +360 recorded
frames, screenshot readback absent, 360/360 nonzero draw frames in every sample.
VSync requested Off and actual Immediate; drawable 1280×720, bloom and Full
lightmaps/reflections unchanged. These are CPU/submission/event-loop timings,
**not GPU execution or minimum frame-rate measurements**.

| View | Stage 2 → Stage 3 median render CPU ms | Stage 3 frame p95 ms | Draws | Stage 3 peak RSS MiB |
| --- | --- | --- | --- | --- |
| Entities | 5.893 →3.079 | 14.017 | 17 | 429.8 |
| Hall | 2.825 →2.863 | 13.830 | 8 | 430.4 |
| Room | 4.069 →2.613 | 12.999 | 31 | 430.3 |
| Window | 2.671 →1.758 | 13.643 | 18 | 430.5 |

Desktop pacing and warm driver caches can dominate these short campaigns. There
is no measured GPU speedup and no change to limits justified by these medians.
Normal release resources and actual submitted counters remain in the CSV/logs;
the stage does not introduce frame-time GI. Diagnostic gathers and PNG/raw exports
are excluded from normal timing. Earlier candidate measurements and unsuccessful
development commands remain preserved under their original names and are not
silently substituted for the sealed receipts.
