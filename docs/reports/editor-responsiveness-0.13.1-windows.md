# Editor responsiveness and ownership — Windows, 30 October 2026

The measured authoring stalls came primarily from repeated exact collision
derivation and duplicated candidate validation. This batch removes that work,
moves history ownership instead of cloning it, and adds explicit diagnostics.
Lantern CPU wall edits fall from about 96 ms to 28 ms; native retained-edit
handlers fall from 62 ms to 37 ms. Cold driver/install stalls remain.

## Source, fixtures and measurement

Baseline is `6da262776d21de6141b474f427d56f63401970a8` plus opt-in timers
and the same developer document probe; no behavioral optimization. Final source
is the commit containing this report. `baseline-bin`, `final-bin` (timing cohort)
and `sealed-bin` (final artifact) retain executable, DLL and source hashes.
The timing and final source differ only in restoration of the original UTF-8
door error-message literal; `timing-to-sealed-source.json` records that boundary.
All final Rust gates and additional native replays use the corrected source.

Host: Connors-Razer-14, Windows 10 Home 19045, Ryzen 9 5900HX (8 cores/16
threads), 16,537,948,160 physical RAM bytes, RTX 3070 Laptop GPU, NVIDIA driver
32.0.16.1714, Direct3D 12, Rust/Cargo 1.99.1 locked release/all features.
Native warmed drawable is 1280 × 800; initial bootstrap starts at 1440 × 816
before the script resize. Both cohorts use the same sequence, actual Immediate
presentation, V-sync off and performance-test sleep disabled. These are operation and ownership measurements;
no FPS throughput gain is claimed or inferred from presentation settings.

Fixtures are byte-matched isolated copies of `projects/performance-lab`
(`performance_lab`, 117 objects) and `projects/environments`
(`lantern_hollow`, 1,271 objects), plus the retained accepted native room/wall
authoring fixture. Every replay records input file hashes, binary SHA, full
command, actions, source checkpoints, frame captures and memory samples.
Builds and CPU profiles use BelowNormal; native UI uses Normal. Native campaigns
are serial with no concurrent build or CPU profiling.

CPU results pool three fresh processes, each with 24 samples per operation,
then 64 alternating changes/undo/redo, save/reopen and switching. Native handlers
include ordinary input dispatch, checked document mutation and preview request;
they exclude asynchronous worker preparation and eventual upload completion.
Native `wait_idle` and generation checkpoints separately verify final display.
Nearest-rank tails at n=64/72 are sample observations, not distribution guarantees.

## Implemented changes and safeguards

- A validation call lazily owns one exact collision-AABB derivation, shared by
  door and route checks, then drops it. Empty door consumers avoid preparation.
  No cross-revision cache, algorithm change, new worker or raised budget exists.
- Wall, room-edge and generic drag previews keep the typed result of their full
  validation. Checked commit gateways still validate the raw authoritative JSON.
  Numeric wall commits no longer validate the same proposal twice. Safety's
  public candidate API still returns fully validated proposals. No typed result
  is retained across document revisions or used to bypass a release/apply check.
- Undo/redo validates before moving its cursor, then swaps current JSON ownership
  into the opposite stack. Actual allocation charges follow the moved value.
  Existing 64-revision/32 MiB per-stack and 256 MiB aggregate admission remain.
- Opt-in phase timers, preview generation/accepted/stale counts and scoped worker
  source-image/model byte diagnostics expose the actual ownership boundaries.
  Existing one-active/one-supersedable-pending worker, cancellation/join, weak
  oversized ownership and worker-backed Safety analysis remain in place.
- `editor-profile --document-session ISOLATED_PROJECT LEVEL_ID` is explicit
  developer tooling, mutates only its supplied disposable project, and is never
  called by tests/CI. It performs no full-map bake or GPU work.

Diagnostic attribution: Lantern door and route phases were 21.621 + 21.763 ms;
after sharing they are 21.893 + 0.158 ms. Serialization (~0.52 ms) and typed
deserialization (~1.21 ms) were not dominant and their byte/schema safeguards
remain. History restore falls from 1.262 to 0.207 ms; subsequent install/enumerate
from 1.376 to 0.835 ms in the diagnostic trace. Timed conclusions below use quiet
CPU runs rather than verbose attribution output.

## Matched CPU edits

Values are median/p95/p99 milliseconds, n=72 in each cell.

| Fixture | Operation | Before | After |
| --- | --- | ---: | ---: |
| lab | wall_move | 0.830/0.981/1.062 | 0.500/0.593/0.641 |
| lab | wall_resize | 0.819/0.910/0.937 | 0.500/0.555/0.601 |
| lab | light_move | 0.478/0.532/0.566 | 0.422/0.500/0.536 |
| lab | material | 0.462/0.551/0.629 | 0.434/0.558/0.671 |
| lantern | wall_move | 96.055/100.849/104.139 | 28.027/29.582/36.690 |
| lantern | wall_resize | 96.481/100.977/103.341 | 28.340/30.117/30.593 |
| lantern | light_move | 48.867/50.909/59.721 | 27.665/30.177/32.871 |
| lantern | material | 49.566/50.995/52.926 | 28.193/28.952/29.809 |

The complete Lantern CPU trace falls from roughly 28.8–29.5 s to 14.6–14.7 s.
Small-map material tails rise slightly; this batch does not establish a uniform
small-map improvement. Full +X door movement is **not run**: no valid authored
candidate exists in these fixtures. Real Lantern door-width editing is covered
separately, including exact undo/redo and save/reopen.

## Native Windows input, interruption and readiness

The repeated-history sessions use 64 alternating light changes, 64 undos and
64 redos, preview supersession, return to idle and process exit. Lantern also
traverses all nine maps twice (18 switches). Each session lasts about 100–137 s;
this is a bounded repeated-flow qualification, not an hours-long soak.

| Fixture | Native handler, n=64 | Before median/p95/p99 ms | After median/p95/p99 ms |
| --- | --- | ---: | ---: |
| lab | retained_edit | 1.541/1.948/2.004 | 1.640/2.128/2.657 |
| lab | history_undo | 1.482/1.922/1.992 | 1.424/1.753/2.002 |
| lab | history_redo | 1.530/3.066/3.503 | 1.466/1.759/1.884 |
| lantern | retained_edit | 62.110/67.449/68.701 | 36.569/42.835/43.566 |
| lantern | history_undo | 60.935/65.132/68.270 | 33.118/35.609/36.387 |
| lantern | history_redo | 63.139/67.153/68.304 | 32.435/34.236/35.056 |

Native single-operation observations (n=1): Lantern wall move 102.72 → 29.13 ms,
wall resize 109.92 → 32.89 ms, door resize 55.82 → 31.38 ms, light move
56.21 → 31.78 ms, material 55.53 → 33.64 ms and Save 119.79 → 74.82 ms.
These single samples do not establish tails. Map-switch dispatch median/p95/max
is 1.431/131.388/131.388 → 1.547/129.663/129.663 ms; asynchronous preview
completion is separate and no map-switch speedup is claimed.

Request-to-visible-preview-ready remains a separate cost. At the matched restore
checkpoints, Lab is about 82–87 → 80–90 ms. Lantern wall move/resize restore is
1.637/1.569 → 1.560/1.448 s, door/light/material restore is
2.279/2.432/2.591 → 2.416/2.562/2.818 s, and rapid final restore is
1.474 → 1.597 s (single observations). Preview worker preparation and incremental
GPU installation still dominate these intervals. This batch establishes reduced
synchronous handler blocking, not a general end-to-end preview latency gain;
`preview-ready-comparison.json` retains each preparation/install breakdown.

Actual wall-body, endpoint and room-edge pointer drags enter preview and commit
paths, cancel without changing source, apply one coherent history revision,
undo exactly and redo exactly. Captured wall/room geometry and Lantern native
frames were visually inspected. The small fixture's wall-move commit sample
rises from 1.149 to 2.198 ms; n=1 is correctness evidence, not a reliable gain.

Fresh-process readiness is measured separately below (one observation each,
±100 ms checkpoint polling). OS storage and driver caches were already warm;
these are not power-on/cold-cache measurements. The earlier long-session cohort
reported request-to-preview-ready around 9 s Lab and 12.2 s Lantern, demonstrating
substantial startup variance unrelated to warmed document handlers.

| Fixture/cohort | Launch to ready checkpoint, s | Preview request to ready, s | Maximum initial upload handler, ms |
| --- | ---: | ---: | ---: |
| lab before | 6.554 | 4.350 | 8.78 |
| lab after | 6.354 | 4.729 | 8.44 |
| lantern before | 8.876 | 7.228 | 1128.63 |
| lantern after | 9.582 | 7.957 | 2107.26 |

Later warmed Lantern preview upload maximum is 17.064 ms (p95 4.460 ms), while
initial install can exceed two seconds. Cold readiness and a hard UI-handler
budget remain unresolved; no chunking or GPU architecture change is claimed.

The timing campaign passes 258 source/state assertions. Additional
final-binary replays pass 174 assertions, including a live/pending
preview at close, clean cancellation/join and reaped processes. Checkpoints prove
real edits, exact authoritative JSON and savepoint behavior, stable selection
semantics, exact saved/reopened edited source and only the latest accepted
generation displayed at idle. Existing lock, invalid-candidate, focus and history
regressions pass. No full Build/Play/bake is rerun in this scoped batch.

## Resource ownership and limits

All values below are independent MiB scopes. RSS/private peaks are sampled with
PSAPI every 100 ms; live/cache/high-water diagnostics describe owned resources.
They overlap and must not be added together or called exact physical VRAM.

| Repeated native session | OS peak working set | Peak private commit | Idle document/history heap charge |
| --- | ---: | ---: | ---: |
| lab before | 374.28 | 536.84 | 8.686 |
| lab after | 372.37 | 532.28 | 8.692 |
| lantern before | 1394.34 | 2116.89 | 107.889 |
| lantern after | 1418.39 | 2145.84 | 107.955 |

Lantern's first and second complete map rounds and subsequent idle have exactly
the same document charge: 113,198,830 bytes, nine documents, 64 retained revisions
and 15,178,068 serialized history bytes, below aggregate admission. Baseline charge
is 113,129,902; ownership transfer retains original allocation capacity instead of
clone-shrunk capacity. This small increase is expected accounting, not a leak.
Unsaved documents/history are intentionally pinned and are not silently discarded.

Final timing-cohort worker: generation 220, 38 accepted responses, 63 obsolete
responses discarded, no active or pending request at idle, displayed generation
220. Worker cache: 56 source images, 173,867,008 pixel bytes, of which 110,886,912
bytes have only cache ownership at that observation, within existing 256 MiB
optional retention. Model storage: 63 models, 13,823,936 geometry/skin bytes,
8,962,048 texture bytes and zero encoded input bytes. These cache scopes overlap
active materials and renderer ownership. Different supersession timing can warm
different source-image subsets; the baseline's 66 entries do not establish a
memory saving.

Both cohorts end with the same HAL live buffer/texture bytes:
179,372,032 / 143,785,984; observed peaks 343,080,960 / 167,116,800. The unique
renderer texture-cache texels are 93,061,048 bytes across 60 persistent entries,
with zero idle-only cache sheets. Driver allocation overhead is unavailable.
GPU live ownership falls from transient peak after supersession/map replacement;
no refused installation is recorded. Dropping the existing preview owner cancels
its budget, sends shutdown and joins its single worker; the final busy-close
replay exits successfully and all owned native processes are reaped.

No concrete over-budget or orphan-retention defect was reproduced. This batch
does not claim lower RSS/VRAM or exhaustive release accounting after every owner,
nor does a stable two-round trace establish hours-long behavior.

## Gates, artifacts and package impact

PASS: final strict workspace/all-target/all-feature Clippy, locked release
workspace/all-feature build, release Clippy, formatting, 400 Rust tests, 372
Python tests (18 expected skips), asset validation (674 assets, 417 placeables,
10 themes, zero warnings), native state/visual/repeated/cancel/save/reopen checks
and patch whitespace. One narrow new regression protects shared door/route
geometry, first-error order and fresh edited geometry; a tiny room/wall/door/route
fixture costs milliseconds and no asset load/GPU/bake. Existing history-charge
and editor phase regressions were adapted rather than duplicated.

The final compiler preparation fingerprint is `95dc8ac8070e637404446958a88eafb1c5803d1859587018452769ed31739b17`.
Shared engine validation changes conservatively affect currency even though no
format, authored geometry, artwork, package layout or compiler bake algorithm is
changed. No map is rebaked here. Required current-package refresh stays in the
final packaging queue. The unrelated dirty menu `scene.bin` remains unstaged
with SHA256 `93b92d7d6caaa9a7fc1a1bec2bb0805c17cccd25f584c730ef882ffbbcf9a27a`.

Evidence and preserved binaries live outside `target` at
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-30/task-2/editor-evidence/`:
`campaign-summary.json`, `campaign-assertions.json`, `sealed-native-assertions.json`,
per-run scripts/identities/timings/source/PNG/memory files, attribution logs,
`hardware.json`, all gate logs/results, hash manifests and `handoff.json`.
Campaign helper scripts sit in the parent task directory. Preliminary native
`interaction-01` used a lost selection and is excluded; early overlapping
CPU/native exploratory samples are also excluded. The report uses serial
corrected `before/after-02`, quiet CPU `01–03`, fresh-process native `03`, and
final-source native `04` verification.
The first final busy-close recipe toggled preview eight times and had already
drained at the close checkpoint; it is not interruption evidence. The corrected
single-request replay requires active/pending work at close and a clean exit.

## Follow-on implementation queue

1. **Cold preview install stalls — pending.** Repeated initial Lantern handlers
   reach 1.1–2.1 s. Attribute driver/resource creation separately from bootstrap
   and preparation, implement a warranted bounded improvement, then demonstrate
   matched readiness/input-tail improvement plus interrupted-install cancellation
   and identical output. This stays in the later startup/GPU queue.
2. **Remaining exact collision derivation — pending profiling.** One roughly
   22 ms derivation remains per required full validation. Any future dependency
   reuse must be bounded, retain all malformed/changed-geometry checks and prove
   a native benefit; no speculative cross-revision cache is added here.
3. **Direct map selection — pending usability work.** The existing Next control
   requires visiting all nine maps to return to Lantern. Provide direct selection
   with preserved dirty/history state, locks and supersession, and test interrupted
   switching without forcing intermediate previews.
4. **Hours-long/platform acceptance — pending.** Extend owner accounting and
   mixed authoring soak beyond minutes. Linux installed-editor acceptance remains
   unrun; Mac remains deferred. Broader #29 baseline/package criteria remain open.

The document pipeline, candidate revalidation and history ownership queue items
are implemented and qualified within the Windows scope above. Resource lifetime
is a bounded-flow pass with explicit unverified longer/platform scope, not a
manufactured leak fix or closure of the aggregate issue.
