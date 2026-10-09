# Stage 7 — integration costs and measurement limits

Recorded 2026-10-09 UTC. The complete package/resource inventory and paired
hero capture measurements below are complete. Affected native follow-up and
publication remain pending; no capture timing is promoted to gameplay FPS.

The development host is an Apple M2 Pro with twelve CPU cores and 16 GiB memory,
using macOS Metal, Rust 1.99.0 and SDL 3.4.18. Normal compiler jobs use twelve
workers and run exclusively; no simultaneous native or other build workload is
used to establish these costs. Source inputs are pinned by
[freeze v8](source-input-identities-v8.json).
The later [freeze v9](source-input-identities-v9.json) changes only two test
expectations; it independently confirms identical ordinary compiler and C2
player bytes. It does not change the measured production behavior.
[Freeze v10](source-input-identities-v10.json) adds only the three corrected
Python expectations and preserves every earlier production identity. The normal
full desktop gate passed in 1,119.500 seconds, including validated unchanged
package reuse. This gate duration includes tests and native controls; it is not
a bake-only or gameplay performance comparison.
[Freeze v11](source-input-identities-v11.json) adds only the ignored directional
GPU test's correct linear/sRGB reference. Original and corrected native PNGs are
byte-identical; strict debug/release Clippy passes and production binaries remain
unchanged. This test correction establishes no performance improvement.

## Dense fixture: lossless storage and actual compilation

All 5,312 props, 48 models, 2,760 solid props, 120 fixtures and 18 routes remain.
The source SHA is unchanged. The original compiler's ordinary combined build
failed the unchanged 1 GiB aggregate safety bound. Its separately built quality
references provide exact streams for comparison, rather than pretending that
the failed combined archive existed.

| Quantity | Original measured records | PLMP6 measured records |
| --- | ---: | ---: |
| Prop bytes per variant | 473,890,078 | 340,854,707 |
| Ordered indices per variant | 6,923,034 | 6,923,034 |
| Input vertex slots per variant | 6,665,147 | 6,665,147 |
| Stored vertex slots per variant | 6,665,147 | 6,297,672 |
| Combined uncompressed package | 1,435,049,755, rejected | 1,035,943,642, accepted |
| Physical accepted archive | unavailable | 334,585,036 |

The accepted package has 37,798,182 bytes of aggregate headroom. Expanded literal
prop records remain 448,534,303 bytes each, below the unchanged 512 MiB guard.
Both encoded and expanded representations are bounded before allocation.
Independent Off/Medium/Full comparisons prove every ordered 69-byte corner and
the complete predicted wire streams exact. See the
[storage audit](integration-capacity-storage-audit.md) and its raw receipts.

The actual forced combined build took 303.866 seconds; building, required
currentness and decoding together took 313.740 seconds. The original separately
built references took 315.231 seconds including their validation, a different
workflow. These numbers do not establish a paired bake-speed improvement.
Two-second samples observed a maximum compiler RSS of 2,448,932,864 bytes across
151 samples. This is sampled process residency, not an exact process-tree peak
or native/GPU memory measure. Actual samples and sampler scope are retained in
`performance/capacity-plmp6-rss-v1-samples.jsonl` and execution receipts.

## Prepared atlas bounds preserve existing illumination

Hallows Full's unchanged 245,495 charts require 9,549,535 padded texels. Nine
pages cannot contain that area; the existing packer succeeds in ten with
disjoint gutters. Its base HDR GPU atlas storage is 160 MiB, with 320 MiB of CPU
arrays. Lower quality profiles retain their existing eight-page policy.
The [planning audit](integration-hallows-atlas-audit.md) preserves the prior
eight-page overflow failure and measured bounded correction.

Demo's Full atlas has ten pages and two illumination groups: forty RGBA16F
layers, 320 MiB GPU payload and 640 MiB CPU arrays. Its 335,544,516-byte KTX2
record needs the corrected generic typed allowance of 320 MiB +64 KiB.
Ordinary entries remain 256 MiB, mesh/prop entries remain 512 MiB and aggregate
packages remain 1 GiB. Density, gutters, content and switch groups are retained.
The [record audit](integration-demo-atlas-record-audit.md) explains the exact
header and payload. These atlas allocations do not estimate total residency.

## Earlier performance contracts retained

Stage 6's 34.6% improvement applies only to its isolated moving-caster CPU
kernel: 8.311 to 5.434 ms over three alternating pairs. All 32 receivers still
refresh globally; static atlases remain baked during motion and skinned shadow
bounds use proxies. It is not whole-engine FPS or GPU evidence. Its before and
after Metal traces had different acquisition/concurrent encoder contexts.
[Stage 6 performance](../stage6/performance.md) retains those limits.

Stage 5's 0.897 to 0.963 ms encoded GPU captures include readback context;
its zero-draw ordinary-window measurements were rejected. No gameplay FPS
claim is inherited from them. Final Stage 7 counters and same-context hero costs
below use actual runs; total driver residency and gameplay stalls remain unmeasured.

## Final paired hero capture and current inventory

The [paired native report](performance/hero-paired-final-v1.json) retains the
actual Stage 6 isolated runtime and current runtime identities, untouched raw
840-frame capture archives per case, command receipts and owned-process cleanup.
Both show 43 draws, 20,832 visible indices (6,944 triangles), 48 total batches,
1,639,320 vertex-buffer bytes and 44,100 index-buffer bytes on every measured row.
No content or quality reduction was used. All exported Metal command-buffer
error tables are empty.

Across one sequential pair, CPU update/render means were 0.068/0.354 →
0.074/0.371 ms. Capture/readback presentation blocked about 81 ms per frame;
the CSV's embedded FPS values are therefore unsuitable for gameplay claims.
The six-second Metal window contains 74+74 versus 72+72 classified surface and
capture scene encoders. Surface-scene medians were 0.957 → 1.023 ms; capture-scene
medians were 1.662 → 1.893 ms. Mixed GPU occupied time per encoder was
1.029 → 1.065 ms. This single acquisition pair establishes descriptive costs,
with measurement variation, and no speedup attribution.

Metal allocation time medians remained 176,472,064 bytes and observed peaks
184,156,160 bytes. These allocations do not represent total process or driver
residency, a plateau or a leak test. Actual `/usr/bin/time` process RSS and
footprint values are preserved separately in the report.

The hero physical archive is 6,599,506 bytes versus Stage 6's 6,632,335 bytes;
its source and visual layout remain unchanged. The current
[48-map resource audit](validation/all48-resources-final-content-v1.json)
uses the two repaired packages and retains every role-specific and aggregate
bound. The affected two sources also pass forced full-build agreement and
unchanged safe reuse; the earlier ten installed-map full/reuse comparisons
remain accepted rather than being repeated.

## Supported-map runtime memory observations

The [affected native receipt](validation/native-affected-execution-v2.json)
retains 58 measurement receipts, 27 with exact owned-native process observations,
and no sampler errors. The largest two-second sampled native RSS was
3,293,020,160 bytes during Pool quality cycling; that command's OS maximum RSS
was 3,445,620,736 bytes. This is process residency in a capture/diagnostic run,
not exact GPU or driver memory, a leak plateau, or a comparison with the earlier
failed chair allocation.

The original 13-map journey passed actual scene-presentation and resource-state
checks. Its OS maximum RSS was 2,408,808,448 bytes; direct native RSS samples
were unavailable, so no plateau claim follows. All owned native groups and
samplers were joined after both allocations. Loading transients and physical
display cadence remain unmeasured.
