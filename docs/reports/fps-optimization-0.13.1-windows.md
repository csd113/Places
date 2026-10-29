# Windows opaque-model shader optimization - 0.13.1

The implementation separates model fragment programs from the architectural
editor ceiling-cutaway discard. Model uniforms cannot carry that flag, but the
previous shared shader still contained a possible discard. A pipeline constant
now removes that unreachable branch for opaque instanced and ordinary model
programs, including ordinary/reflected facing variants. Non-instanced,
two-sided opaque models receive matching scene/emission programs.

This preserves every authored face, vertex/index range, texture, material value,
lighting payload, alpha threshold, blend order and reflection setting. The three
tree GLBs and five cabinet GLBs actually use opaque, double-sided materials;
back-face culling would not honor their present contract. No model/material
sidedness or artwork is changed. Shared prop geometry and material binding
caches already existed and are retained.

The measured change is shader specialization. Its motivation is to permit
earlier depth writes/rejection in opaque model programs. No hardware early-depth
counter was collected, so the precise hardware scheduling mechanism remains an
inference; the GPU timings below measure the whole rendered workload.

Baseline source is `c9c9eb92add0e5182314ebd3f1764e4ccf96a32c`, after the UI,
startup and checkpoint work, with exact local/remote match and a clean checkout
before editing. Its CI 40922119422 passed. Baseline player SHA-256 is
`319dd6994189a532ca2d02cb6c287392c19e04281e0bbf05bb7adca1b6228edd`.

Final player SHA-256 is
`037737d5ee890a52560353a4ea862c237e613aa14e122331cbab707301e5dbb5`.
Frozen baseline/final executables, required DLLs, raw evidence, failed attempts
and runnable recipes are retained outside `target/` under
`local/verification-0.13.1-optimization/verification/`. The original working
copy is `Documents/Codex/2026-10-29/task-3/`. This report records the final
opaque-only specialization, not the broader exploratory shader variant.

## Matched throughput results

Three independent pairs per camera produced 18 qualified runs, each with 1,200
measured frames (21,600 total). Values below are medians of run-level values.
FPS is `1000 / mean frame-loop ms`, not the reciprocal of a median frame.
Pair order is baseline/final, final/baseline, baseline/final across the rounds.

| Camera | Average FPS, baseline to final | GPU elapsed mean ms | Frame p95 ms | Frame p99 ms |
| --- | --- | --- | --- | --- |
| Hollow trees | 58.20 to 64.07 | 16.426 to 14.818 | 21.337 to 18.698 | 23.644 to 21.007 |
| Close Arcade cabinets | 105.14 to 131.32 | 8.924 to 6.826 | 12.016 to 10.162 | 12.392 to 11.007 |
| Arcade lanes / Full planar reflection | 102.74 to 123.12 | 9.191 to 7.341 | 12.166 to 11.632 | 12.649 to 12.368 |

Paired changes are calculated within each pair before taking their median;
therefore they need not equal differences between the medians above. Brackets
show the complete three-pair range, final minus baseline. Negative time is good.

| Camera | FPS change | Mean loop change ms | GPU mean change ms | p95 change ms | p99 change ms |
| --- | --- | --- | --- | --- | --- |
| Hollow | +5.88 [+4.64, +5.99] | -1.577 [-1.632, -1.137] | -1.608 [-1.631, -1.174] | -2.639 [-2.815, -0.924] | -2.302 [-2.637, -2.217] |
| Cabinets | +26.18 [+21.72, +41.57] | -1.896 [-3.455, -1.540] | -2.098 [-3.540, -1.724] | -1.824 [-2.509, -1.814] | -1.385 [-2.981, -1.003] |
| Lanes | +18.62 [+17.13, +22.62] | -1.493 [-1.828, -1.288] | -1.746 [-2.043, -1.387] | -0.554 [-0.934, +0.904] | -0.277 [-0.313, +0.394] |

All nine pairs improve average throughput and GPU elapsed mean. All six
Hollow/cabinet pairs also improve p95 and p99. One cabinet baseline is slower
(90.85 FPS versus 105.14/108.42 in the other rounds); it is retained, with the
resulting broad range visible above. The lanes tails are mixed, so this does
not establish a tail improvement for that view or solve the earlier planar
reflection tail finding. No outlier has been silently discarded.

CPU update/render means, respectively, remain approximately 4.49/2.17 to
4.51/2.16 ms in Hollow, 1.30/1.72 to 1.35/1.72 ms close to cabinets, and
1.36/2.40 to 1.37/2.35 ms in the lanes. This is predominantly a GPU-side gain.
It is smaller than the previous content-removal ceilings, which are diagnostic
attribution controls and are not implemented gains.

GPU and CPU tails are checked separately. These are run-level medians, ms,
baseline to final; CPU update/render spans overlap asynchronous GPU execution.

| Camera | GPU p95 | GPU p99 | CPU update p95 | CPU render p95 |
| --- | --- | --- | --- | --- |
| Hollow | 18.748 to 16.476 | 19.275 to 16.630 | 4.863 to 4.886 | 2.765 to 2.772 |
| Cabinets | 9.625 to 7.918 | 9.724 to 8.148 | 1.872 to 1.925 | 2.180 to 2.229 |
| Lanes | 9.975 to 8.594 | 10.225 to 8.667 | 1.959 to 1.936 | 3.040 to 2.962 |

Hollow paired GPU p95/p99 changes range from -2.719 to -1.311 / -2.834 to
-1.377 ms; cabinets from -3.920 to -1.108 / -4.199 to +0.019 ms; lanes from
-1.674 to -1.322 / -1.925 to -1.387 ms. The cabinet p99 difference of +0.019 ms
in one pair is retained. Lanes CPU tails vary more than their median suggests:
paired update p95 spans -0.050 to +0.702 ms, render p95 -0.484 to +0.643 ms.
No consistent CPU improvement is established, and lower GPU tails alone do not
establish better full-frame tails in the lanes.

## Capture conditions and interpretation

Accepted performance runs use native Windows/Dx12 on the Razer RTX 3070 Laptop
GPU, Normal process priority, actual Immediate presentation, swap interval zero,
1920x1080 physical drawable, FOV 60, High textures, High/16x filtering, Full
lightmaps/reflections, bloom on and V-sync off. No frame limiter is enabled.
Every pair uses the attribution report's exact camera, 1,800 warm-up and 1,200
measured frames with a fixed 1/60 simulation step and real wall-clock timing.
The fixed simulation step is not a rendering cap. Background builds use
BelowNormal priority and run separately from all native timing.

The host display is 2560x1440 at 165 Hz; the recorded physical drawable is
1920x1080. Accepted mean FPS spans 57.66-132.42 and changes with workload,
well below a 165 Hz plateau. Actual negotiated Immediate mode and swap interval
zero are verified in each native log, alongside disabled V-sync/limiters. These
observations support an uncapped throughput comparison on this host.

| Level / camera | Render eye | Yaw, pitch |
| --- | --- | --- |
| lantern_hollow / trees | 18, 1.6, 21.6 | -35, -2 |
| arcade / close cabinets | 7.4, 1.6, 17.8 | 306, 5 |
| arcade / lanes | 6.8, 1.6, 28 | 25, -2 |

GPU samples deduplicate asynchronous sample IDs. Hardware timestamp spans
include scheduling stalls and overlap CPU work; they are not active utilization
and must not be added to CPU time. Native foreground ownership is sampled every
250 ms; accepted measured periods require at least 99%. Agent activation/F12 is
performed during warm-up, with no timed gameplay input.

The first baseline cabinet attempt had zero foreground ownership and is retained
but disqualified. The first candidate build had a const enum-comparison compile
error and is retained. The corrected candidate pilots are screening only,
separate from final confirmation. Run selection uses the earliest qualified
run per camera/control/round, never the fastest successful replay.

The broader pilot also specialized cutout/blended programs. It increased first
pipeline preparation substantially without a justified benefit for those
coverage-dependent paths. The final change specializes only opaque model
scene/emission programs and leaves their existing coverage programs intact.

## Loading tradeoff

These are cold-process launches with natural OS/driver caches, not a cache-purged
startup benchmark. In the same final paired cohort, median extra shader-worker
preparation is 1.679 s for Hollow, 1.229 s for cabinets and 1.284 s for lanes.
That worker overlaps other preparation. Median request-to-playable increases
are 0.279 s [0.168, 0.422], 0.508 s [0.332, 0.726] and 0.546 s [0.138, 0.915],
respectively. Median process-to-playable changes are +0.359, +0.548 and +0.604 s,
with wider launch variation. The loading event-pumping worker remains in place;
this phase does not make a new input-latency claim.

## Quality and validation

Fourteen final-composition PNGs compare seven matched conditions at 1920x1080:
the three performance cameras, pool water/glass/characters, Arcade blacklight
switched off, and a close translucent Hollow ghost at 5.000 and 12.017 simulation
seconds. Five pairs are RGBA pixel identical. Each ghost pair differs in only
18 pixels (0.00087%), maximum one channel level out of 255, with no pixel over
that error. Manual inspection retains foliage silhouettes, cabinet CRT/artwork,
emission, reflective lanes, pool transparency and the ghost's visible interior.
The two ghost times differ by 44,726 pixels on both builds; the blacklight switch
changes 791,384 pixels on both. Logged light states and animation times agree.
These are bounded native quality witnesses, not a proof for every possible map.
Capture-only launch timings are excluded from FPS acceptance.

The final exact source passes:

- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- The same strict Clippy command with `--release`.
- `cargo build --locked --release --workspace --all-features`.
- `cargo test --workspace`: 390 passing Rust tests.
- Python discovery: 371 checks, 18 expected skips, with `PLACES_SKIP_SMOKE=1`.

No test is added: existing shader/pipeline and material contracts plus explicit
native Windows rendering cover this bounded program specialization. Ordinary
test/CI commands do not bake shipped maps. Native smoke is covered by the runs
above. The Python runner initially lacked Git's `sh.exe` on PATH; the failed
receipt remains, followed by a complete pass with runner-local Git bin/cmd PATH.
Initial compile/Clippy failures and the broader shader pilot are retained.

Separate logging-enabled diagnostic runs collect three CPU-stage samples and
five GPU-pass samples in each measured window. Hollow character lighting mean
is 3.108 to 3.154 ms; skin upload 0.820 to 0.617 ms. Three samples do not establish
a skin-upload gain. The character-lighting floor is still present. GPU scene
pass means are 15.501 to 13.121 ms in Hollow, 5.675 to 4.239 ms for cabinets and
5.776 to 4.180 ms in the lanes. Full planar pass means are 1.922 to 1.796 ms for
cabinets and 2.829 to 2.041 ms for lanes. These sparse, overlapping timestamp
spans support the direction of the shader result; they are descriptive and are
excluded from the authoritative FPS cohort. They do not independently price a
new planar optimization or prove the hardware early-depth mechanism.

`places-compile validate` re-hashes and accepts the retained environments archive
(393 dependencies intact, no warnings). `verify --require-current` for Hollow
and Arcade exits 1 with `integrity_valid=true`, `bake_required=true`, solely
because compiler preparation code changed. This is the expected conservative
engine-source fingerprint result; the archive remains valid for runtime loading.
An initial isolated validation invocation omitted `PLACES_ASSET_ROOT` and failed
to find the installed audio catalog; it is retained as a setup failure. The
correctly configured invocation passes. No stale result is called current.

Evidence indexes are `confirmed-analysis.json`, `confirmed-loading.json`,
`qualified-run-validation.json`, `visual-comparison.json`,
`visual-witnesses.json`, `diagnostic-analysis.json`, `currency2/result.json` and
`final-gates3/complete.json`. Each native directory retains its raw CSV/log,
foreground samples, environment/settings, process identity and receipt. Frozen
supervisor source/heartbeat/result files document independent job lifetime;
all successful and failed paths remain available for review.

Current source maps, assets, PNGs and prepared archives are unchanged by this
work. Comparisons deliberately reuse the same retained archive payloads on both
sides. The environments archive SHA-256 is
`749c9f3e45120d7c72416891dd5d4f5965739dc88b60b5827f06fb1e33ac28f5`.
Developer preparation provenance is reported separately from runtime archive
identity; no stale-check suppression, provenance relabelling or blanket shipped
map rebake is used. Packaging refresh remains a later queue step.

This change does not implement generic character-lighting caching, opaque draw
sorting or a reflective-footprint scissor. Character lighting already has exact
dependency keys, dirty-region checks and cached static rays. The Hollow CPU
floor remains a separate target. Full planar reflections stay enabled; any
wide-view improvement here comes from model programs used by the existing
capture, not an independently priced planar optimization. Smaller screening
categories are not promoted into new work without evidence.

Native Mac acceptance remains deliberately deferred until requested. The
measurements describe this host with natural clocks, not hardware-general or
peak-device guarantees. This is development-branch work, not a public release.
