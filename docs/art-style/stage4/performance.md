# Stage 4 measured costs — 2026-10-08 UTC

Same Mac 14,9 / Apple M 2 Pro, 12 CPU /19 GPU cores, 16 GB unified memory as
Stage 3. Native jobs and bakes were sequential, with no overlapping build/test
job during measurements. Normal release timings exclude screenshot readback.
Ordinary desktop applications remained running. Requested VSync is Off; these
receipts report actual Metal `Fifo`. CPU timings are submission/event-loop work,
not GPU execution or physical display cadence.

## Compiler and storage

Normal forced build, log/OS receipt,
member comparison:9.769 s compiler /10.06 s OS,
650.5 MiB peak RSS, 5,189,439 bytes. Stage 3 was 9.804 /10.08 s, 646.7 MiB,
5,187,045 bytes. Package growth is 2,394 bytes /0.046%; this single run does not
establish a bake-time or memory improvement. Settings and budgets are unchanged.

Each Medium/Full field retains 495 slots /260 air-valid samples on the 1.5 m lattice.
PLPF grows 17,858→29,758 serialized bytes: an 11,900-byte selected-direct sidecar
(4-byte source count +four 4-byte IDs +495×24-byte means/moments).
Compressed fields grow 8,280→9,468 and 8,403→9,603 bytes. CPU coefficient payload is
calculated from the declared Rust element layouts:495×36 +495×32 +4×4 =33,676
bytes, excluding container/capacity/allocator overhead. Only the active variant
is used by entity sampling. No separate GPU probe lattice is uploaded; entities
receive interpolated coefficients in their 5632-byte environment uniform
(previously 2896 bytes).

Two 1024² atlas pages per Medium/Full variant remain 33,554,432 resident bytes,
33,554,628 stored bytes. Geometry, 5354 charts, 16 prop batches, 23 collision spans,
3194 navigation cells /2 regions remain unchanged in the original hero. PLMP v 5
adds one caster flag byte per batch. No artwork, texture, concept or model changed.
The instrumented probe audit produces a package byte-identical to
the normal build; its timing is excluded from the normal compiler comparison.

## CPU and submitted entities

Normal visible-count samples,
parsed distributions:120 warmup +1000
recorded frames per run. All 1000 frames submit nonzero scene draws. The fixed
cost camera covers the same original hero with additive
ordinary runtime chairs; no existing content is removed. One separate
raw proof and
actual resource/draw receipt verify 32
rigid entities, 32 dynamic draws, one cached 204-triangle mesh/BVH and one model
build (0.097 ms measured preparation). The scene submits 57 draws; submitted
frustum acceptance does not establish pixel visibility after depth occlusion.

| Content | Update median /p95 ms | Render CPU median ms | Frame p95 ms | Scene draws | Peak RSS MiB |
| --- | --- | --- | --- | --- | --- |
| 1 rigid |0.070 /0.362|2.433|14.133|26|431.4|
| 4 rigid |0.122 /0.427|2.426|14.033|29|430.6|
| 8 rigid |0.201 /0.492|2.372|14.366|33|431.5|
| 32 rigid, stationary |0.699 /0.807|1.818|13.074|57|431.2|
| 32 rigid, one moving |7.559 /7.672|0.293|9.092|57|431.5|
| 4 skinned +1 rigid, actor camera |0.824 /1.043|0.497|12.973|33|438.0|

The moving case changes one caster every measured frame, forcing conservative
refresh of all 32 receiver payloads. This is a real cost: about 6.86 ms more median
whole-engine update than stationary. It includes benchmark command dispatch/log
writes, transforms, ray sampling and uniform updates; it is not an isolated
lighting kernel measurement. Stationary frames reuse payloads and never rebuild
BVHs. Eight bounded floor-contact subjects add no shadow draw or texture.
Skinned bodies use bind-bounds floor proxies rather than posed ray geometry.
CPU render decreases under different pacing; this is not evidence of a renderer
speedup or a minimum-FPS claim.

Earlier original-camera count samples retain valid CPU/
resource data, but extra chairs were outside its frustum (17 draws throughout).
They are excluded from GPU scaling claims. The original one-entity view gives
fresh Stage 3→Stage 4 update medians 0.050→0.069 ms and render 2.494→2.207 ms;
raw distributions qualify different frame counts
and desktop pacing. The first attempted GPU attachment ended after the old
player exited; it recorded no GPU data.

## Native GPU execution

Successful eight-second `Metal System Trace` attachments target verified owned
player PIDs. The existing
`tools/bench/inspect_metal_trace.py` analyzes the stable 1–7 s window: union of
active target GPU intervals divided by uniquely labelled scene encoders.
Overlapping Vertex/Fragment channels are counted once. This includes real
scene/post/upload work; it does not isolate one shader, direct light or shadow.
GPU spans and active work are distinct measures.

| Camera/content | Scene encoders in 6 s | Active GPU mean /scene ms | Scene span median /p95 ms | Metal time-median /peak MiB |
| --- | --- | --- | --- | --- |
| Stage 3 original, 1 rigid |1258|0.740|0.498 /0.547|143.6 /143.6|
| Stage 4 original, 1 rigid |1212|1.046|0.953 /1.383|143.6 /143.6|
| Stage 4 wide, 1 rigid |1250|0.959|0.860 /1.066|143.6 /143.6|
| Stage 4 wide, 32 rigid stationary |1150|2.111|2.181 /4.644|144.0 /144.0|
| Stage 4 wide, 32 rigid moving |720|3.567|3.630 /4.490|144.2 /144.2|
| Stage 4 actor camera, 4 skinned +1 rigid |1334|0.999|0.777 /1.203|147.0 /148.4|

The original-camera single pair increases active GPU work by 0.306 ms; no GPU
speedup is claimed. The wide 32 moving sample increases work by 1.456 ms relative
to stationary, including changed uniform uploads. Dense contacts/direct evaluation
and geometry all remain enabled. These bounded samples establish costs for the
hero, not universal device budgets. No limit, quality or content is weakened to
improve a number. Global caster invalidation is a documented optimization
opportunity for the later measured pipeline work.

Failed trace attempts are retained: `before-1` found an already-exited PID;
`actors4-final` rejected an incorrect camera selector before a player started.
They supply no GPU metrics. Corrected successful runs have explicit `trace_exit:0`
and `native_exit:0`; full process-inventory TOC/raw tables stay outside tracked
reports, while summaries contain only the owned Places process.

## Solver-15 final-audit correction

The [zero-source repair](contracts.md) preserves v 3 spatial presence in
new indirect-only and switch-only fields. Those fields now retain `4 + 24*count`
serialized bytes and `32*count` CPU coefficient bytes, excluding containers, where
revision 14 wrote legacy v 2. This cost is derived from the actual codec/layout;
no separate GPU probe lattice is introduced. All five accepted hero controls
already retain selected sources, so their field sizes and runtime shader work
are unchanged. Refreshed package payload equality is checked separately. Existing
CPU/GPU samples describe the accepted shader and payloads; they are not relabeled
as measurements of the refreshed binary.

The conservative global caster invalidation cost remains an explicit Stage 6
profiling lead in the shared ledger. Stage 4 accepts its measured correctness and
cost; no visibility contract is weakened to reduce this number.
