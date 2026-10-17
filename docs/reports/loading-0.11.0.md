# Asset/map loading and resource ownership — 0.11.0 staging

Based on `f11e2819c92ad96488946f0005a4156f380c6621`, on the user's Mac.
This pass follows the completed renderer, shader, sound, caustics and physics
work. It keeps the existing engine/game/compiler boundaries and package format.

## Demonstrated problems and corrections

- **Inconsistent package snapshots.** The preparation key, semantic world and
  variant each opened the path independently. A replacement could combine
  records from different manifests or pair retained gameplay semantics with a
  newly installed lighting variant. Loaded semantics now carry their canonical
  manifest identity. Every variant open and cache reuse checks that identity
  before caching or installation. A changed retained package gives an actionable
  reload error. Generation cancellation, bounded queues and the previous world
  remain owned by the existing preparation worker/frame loop.
  Native acceptance then exposed a recovery handoff defect: the subsequent
  explicit reload could stage new records under the renderer's rolled-back
  configuration. Staging now republishes the player's current graphics
  selection, and a successful world commit clears the previous failure hold.
- **Untrusted allocation hints.** A 10-byte prop header or 66-byte mesh header
  could reserve 1,048,576 in-memory collection entries before checking payload
  length. Readers now prove minimum encoded widths before reservation and charge
  each mesh range against the remaining aggregate geometry budget. Manifest
  lengths must match the archive index; actual decoded lengths must match too.
  Blob names and manifest SHA-256 declarations must agree.
- **Unsafe import replacement.** Direct copying could truncate the destination
  through a same-file alias or leave an installed package partly overwritten.
  Import now uses an exclusively created sibling, bounded copying, flush/sync,
  validation of the staged archive/semantics and atomic publication. Failure
  cleans only the owned temporary and preserves the destination. Its 1,088 MiB
  transport ceiling permits the existing 1 GiB decompressed budget plus ZIP
  overhead. Selected prepared records still receive their normal load-time
  decoding/validation; import does not decode every quality variant.
- **Repeated image work and late budgets.** Warm prefetch decoded unchanged PNGs
  before discovering the cache already owned them. It now skips those decodes
  and stages at most 64 images / 256 MiB decoded. Material resolution returns a
  named error immediately after its existing 1 GiB budget is crossed, rather
  than decoding the remaining table. Fixture sheets share the budget, with
  shared Arcs charged once, and unused shared-family faces skip resolution.
  Failed material resolution discards cache-only pixels; successful and failed
  level loads trim optional CPU image retention to 256 entries / 256 MiB.
  Pack path aliases now resolve to the stored image's identity, sharing one
  decode and budget charge. Active worlds keep independent image Arcs.
- **Malformed asset inputs.** Regular-file reads bound PNGs at 32 MiB, GLBs at
  64 MiB, catalog JSON at 8 MiB and gameplay JSON at its existing 1 MiB limit.
  Read growth is checked. GLB parsing borrows its BIN payload and validates exact
  framing, unique JSON/BIN chunks, one local buffer and declared buffer length.
  Existing geometry ceilings and animation row checks run before component
  allocation; morph resolution reuses the known vertex count.
- **Undercharged retained worlds.** The prepared-cache estimate counted only
  collision walls and eight navigation bytes per cell. It now includes floor,
  ceiling, water and ladder samplers, navigation class/portal/mask/region
  allocations, probe points/chains and spare vector capacity. The existing
  three-entry / 192 MiB cache can evict earlier; active oversized worlds still
  use their independent lease and weak reuse identity.

Installed-level discovery, image ownership and import now live in
`engine/loader/manager.rs`, preserving `loader::LevelManager`. No new executor,
asset framework, dependency, runtime texture imagery or editor module was added.
Physics sampler helpers only count memory; simulation behavior is unchanged.

## Verification

Final verification receipts are retained outside `target` under
`local/loading-0.11.0/`, including commands, gate logs and source compatibility
checks. The source scan inspected all 366 GLBs and 578 PNGs: zero compatibility
findings, largest GLB 2,619,232 bytes, largest PNG 3,326,060 bytes, catalog 310,967
bytes. Asset validation reports 593 assets, 370 placeable, eight themes and zero
warnings. No production artwork or authored map changed.

Nine new CPU regressions use tiny headers, in-memory ZIP/GLB/PNG records,
temporary import files and a four-byte material budget. They protect the exact
allocation, snapshot, alias and failed-publication bugs above; they require no
map bake, native device or shipped asset tree. Native lifecycle checks remain
explicit developer operations outside automatic CI and Rust tests.

The full `RUSTC_WRAPPER= sh tools/check-rust.sh` gate passes: formatting,
all-target/all-feature check, strict debug and release Clippy, `cargo test
--workspace` (51 passed, zero failures/ignores, 0.06 seconds), and the locked
all-feature production build. The nine added regressions account for the
increase from 42. Python source contracts pass 372 tests with four existing
skips in 99.893 seconds. One initial material-budget regression exposed the
real pack alias duplication; its sharing assertion remained unchanged after
the fix. Initial and final logs are preserved separately.

All eleven selected native lifecycle cases pass against the final direct
executable on Apple M2 Pro / Metal in 138.272 seconds. They exercise CPU-ready
staged-upload cancellation, resize/cancel/retry, in-flight reuse, shutdown during
preparation, latest-generation supersession, in-place replacement, retained
lighting refusal and explicit recovery, corrupt-package retry, repeated visits,
graphics-only gameplay preservation and diagnostic material fallback. Twenty
scripted actions have maximum observed enqueue-to-handler latency 8.14 ms;
small-fixture staged disposal measured 0.217 ms. These observations are not
large-level latency bounds. All 141 RSS samples succeeded.

The initial native run passed ten cases and exposed the graphics handoff defect
above; the complete selection was rerun after fixing it. The final receipts are
`native-final/result.json`, per-case logs/traces and `native-gate-final.log`.
The initial `native/` directory remains intact. These small fixture bundles use
the silent audio fallback; production audio ownership is checked separately.

The existing Office and Pool packages then completed eight native commits,
including a deliberately corrupted disposable Pool copy, named load refusal,
restoration and successful retry. Both final captures were inspected. Native
audio initialized with 63 shared clips / 22,065,040 decoded bytes and zero
errors. Each captured world had one live ambience voice; shutdown cleared both
active and pending voices. Package geometry cache hits followed the first load
of each map. Accounted prepared records stayed at 27,988,086 bytes for Office
and 11,723,120 for Pool. Each repeated world texture table stayed at 33,554,424
GPU bytes; later visits uploaded zero world textures and had no missing/fallback
draws. These counters cover the named resource families, not all GPU memory.

Functional runs used actual Fifo presentation at a physical 3024 × 1676 drawable,
with V-sync enabled. No FPS comparison or throughput improvement is claimed.
The first production run's commit RSS ranged from 276.4 to 387.6 MiB and included
framebuffer readback; its short rise alone cannot establish a leak or a plateau.
Raw package hashes, captures, RSS samples, traces and acceptance checks are in
`production/`. A separate repeat control omits framebuffer readback.

That readback-free control completed 31 alternating Office/Pool commits with
29 prepared-cache hits, zero load/GPU failures and 99 successful coarse RSS
samples. Both prepared-record charges and each world's texture residency stayed
constant; every visit after initial uploads reported zero world-texture uploads.
After the first ten commits, commit RSS ranged 355.45–376.41 MiB; the final Office
commit was 363.14 MiB. These are short observations with allocator/driver
residency, not a proof against slower leaks. Its receipts are in
`production-repeat/summary.json` and the adjacent log/trace/memory files.

All four production executables are preserved under `local/loading-0.11.0/bin/`
with SHA-256 receipts. The final game hash is
`68ced53a537eb208e1a2972315cc9fbb67df19b2062097597ed1b7ed569a6df6`.
No shipped map was baked; the production runs used the existing verified
Office/Pool packages. Exact pushed-commit CI and final cleanup/custody receipts
are retained in the same local evidence directory.

## Limits and stage boundary

The budgets account retained payloads and bounded decoding; they are not a
single absolute process/GPU memory ceiling. Displayed/staged worlds, independent
model/animation data, decoder scratch, audio, GPU allocations and driver caches
have their existing owners and can coexist. No new aggregate animation-keyframe
policy was invented. Portable path checks reject ordinary special files before
open, but do not claim protection against arbitrary external filesystem races.
Cancellation is cooperative between package preparation phases; a single
bounded decode is not preempted mid-record.

Native evidence is macOS/Metal only. Windows/Linux acceptance and broader import
UX/distribution work remain with issues #17 and #21, which this pass does not
close. No accompanying loading-only issue was open. No main merge, new branch
or release is part of this stage; settings/input, entity/navigation, editor,
map-format reset, broad compiler redesign and flowing water remain deferred.
