# Stage 5 costs — 2026-10-08 UTC

[Native comparisons](README.md) · [Contracts](contracts.md) · [Handoff](handoff.md)

Measurements use the Apple M2 Pro / Metal, logical 640×360 and native 1280×720,
High lighting/filtering, Full atlases/reflections, fixed exposure 1 and the surface
camera. No quality limit, content or assertion is weakened for a number. Builds,
bakes and tests did not overlap the five traced native samples. These are single
bounded samples, not device-independent budgets or a speedup claim.

## Real GPU work, with an explicit presentation limitation

First normal-window samples completed with zero scene draws. The unchanged surface
acquisition path skipped unavailable frames; telemetry spent time yielding. Those
CSV timings and traces are [rejected](performance/rejected-native-v1.json).
An empty established lighting sequence did not change that outcome. No renderer
fix, artificial foreground event or FPS claim is inferred from these samples.

Replacement samples use the existing native sequence capture path. Each capture
calls the ordinary `encode_frame_into` scene/emission/blur/resolve shaders and
resident targets, then copies and reads back the result. An owned-PID Metal System
Trace records eight seconds; analysis uses seconds 1–7. Window acquisition remains
unavailable or intermittent, so the denominator is uniquely labelled **encoded
scenes**, not presented frames. GPU occupancy includes real capture/copy/driver
work. CPU frame telemetry includes PNG/readback/yield and cannot establish normal
presented-frame CPU cost. No CPU improvement or ordinary gameplay FPS is claimed.

The existing `tools/bench/inspect_metal_trace.py` unions active hardware intervals
so overlapping Vertex/Fragment channels are counted once. Allocation state changes
are weighted by time. [Summaries and pass breakdown](performance/gpu-summary.json),
[owned-PID commands/outcomes](performance/gpu/) and
[export commands](performance/export-commands.json) are tracked. Raw `.trace`, GPU
and allocation XML remain outside `target/` under
`debug-maps/art-style-hero/evidence/stage5-*-capture*-export` and matching traces.
Only one untouched proof image per case is retained; repetitive profiling scratch
PNGs are removed after recording their count. This is not another gallery campaign.

| Native capture case | Encoded scenes in 6 s | Active GPU union /scene ms | Scene span median /p95 ms | Metal time-median /peak MiB |
| --- | ---: | ---: | ---: | ---: |
| Stage 4 corrected surface baseline | 109 | 0.897 | 0.702 /0.778 | 154.594 /161.922 |
| Stage 5 same control | 110 | 0.963 | 0.759 /0.793 | 155.000 /162.328 |
| Stage 5 same control, bloom off | 111 | 0.831 | 0.753 /0.795 | 155.000 /162.328 |
| Authored night + regional haze | 109 | 1.029 | 0.832 /0.986 | 155.000 /162.328 |
| Aurora + severe snow | 159 | 1.418 | 1.229 /1.502 | 163.641 /170.969 |

The paired control increases total active work by 0.066 ms per encoded scene.
Bloom-on versus off differs by 0.131 ms in this sample. Emission/blur/resolve
category active unions are separately inspectable: after control emission 0.085,
horizontal blur 0.043, vertical blur 0.041 and resolve 0.093 ms per encoded scene.
Categories can overlap and must not be summed as total device occupancy. Fog and
transparency shader evaluation remain inside scene/emission cost; they are not
isolated kernels. Night/storm comparisons include intended atmosphere/weather
changes and different submission pacing. The storm cost includes the existing
1400-particle snow budget, not just the global fog controls.

## Attachments and resident resources

At 1280×720 the post chain retains the same two full-size RGBA16F scene/emission
images, Depth32Float scene depth, two 320×180 RGBA16F blur images and full-size
RGBA8 presented image: 23,040,000 bytes (21.973 MiB), excluding main surface/depth,
driver allocation and other scene resources. No thickness/refraction copy or new
full-size target is added. Bloom-off retains the existing resident targets. Two
32-byte authored resolve uniforms replace six: 64 versus 192 bytes. They upload
only when values change. The extra coverage pipeline has driver-dependent storage,
included in the trace allocation rather than assigned an invented byte cost.

The corrected before control has 5597 charts; after has 5598 and 1353 additional
chart texels for the water rectangle. Both remain two 1024 atlas pages / four
RGBA16F layers / 33,554,432 bytes resident. Prepared field slots remain 495.
Normal final capture receipts account 13 visible base-scene draws in the surface
view; this excludes sky, reflection, emission duplicates, post and UI. No geometry
is dropped. Existing changed material reflection eligibility produces two bounded
probe points instead of one in the additive control; this explains part of its
package/resident increase. Original hero remains one probe point.

## Compiler and storage samples

Cold explicit builds use the native frozen compiler, `--force --workers 12`, all
three variants and the full repository asset root. [Commands/results](compile-v3/commands.json)
and JSON/logs retain measured phases, `/usr/bin/time -l` and verification. Every
accepted package passes `verify SOURCE --package PACKAGE --asset-root ASSETS
--require-current --json`. One corrected legacy control is measured with its
compatible old catalogue and compiler; this is not a multi-sample throughput study.

| Package | Compiler ms | Archive bytes | Full charts /probe points |
| --- | ---: | ---: | ---: |
| Corrected Stage 4 additive control | 8001.1 | 5,434,304 | 5597 /1 |
| Stage 5 original hero | 10200.1 | 5,189,438 | 5354 /1 |
| Stage 5 additive control | 8063.2 | 6,020,272 | 5598 /2 |
| Night | 8123.8 | 6,026,934 | 5598 /2 |
| Aurora | 8077.0 | 6,027,165 | 5598 /2 |
| Storm | 8191.7 | 6,027,153 | 5598 /2 |
| Translucent character control | 8184.2 | 6,020,347 | 5598 /2 |

The additive control archive grows 585,968 bytes (10.78%), including the second
reflection payload and changed prepared lighting. Original hero archive differs
from the Stage 4 package by one byte; that is not a storage or compile speedup.
Preparation build times were contended and are recorded only as provenance.
No PNG is created, resized or replaced. Stage 6 retains ownership of measured
global caster invalidation and dependency-aware cache improvements.
