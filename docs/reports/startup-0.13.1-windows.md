# Windows startup responsiveness - 0.13.1

This targeted follow-up to UI commit
`d8c23b506b3e7a2f13bf2fe7adb88d4c1e5269da` keeps the player's event loop
running during first-use world shader compilation. It preserves the preceding
UI implementation, FPS report and all retained evidence. No steady-state
rendering optimization, save/checkpoint work or Hub sign authoring is included.

## Measured result

Three interleaved cold-process pairs use the preserved UI binary as the baseline
and the new release binary as the candidate. The interval includes adjacent
event pumps from the first pump through the first pump after the first playable
world commit. Timed runs contain no GPU readback captures and run after owned
compiler/source jobs exit. OS and driver caches are not purged.

| Pair | Baseline longest gap (ms) | Candidate longest gap (ms) |
| --- | ---: | ---: |
| 1 | 3238.574 | 360.007 |
| 2 | 3381.919 | 228.842 |
| 3 | 3385.665 | 227.833 |
| Median | 3381.919 | 228.842 |

The median paired reduction is 93.23%. This fresh baseline is faster than the
original UI cohort's 3.7-4.1 second gaps, so the paired numbers above are the
comparison. Both use DX12/DXC on the RTX 3070 Laptop GPU, High quality, Full
lightmaps/reflections, Fifo presentation and Normal process priority. Actual
drawables are 960x600 and, during resize, 1280x720 at Windows 150% desktop scale.
These are responsiveness measurements, not an uncapped FPS comparison.

Total entry-to-playable time does not improve: baseline 12.628-12.935 seconds,
candidate 12.941-13.108 seconds. The change makes waiting interactive rather
than claiming less total work. Queued Escape and resize events are handled
during compilation; initial Escape retains the existing rule that the only
first-world request cannot be canceled into an unusable empty background.

## Attribution and implementation

The blocking call was `WgpuRenderer::ensure_world_pipeline` on first
`render_scene`. The retained original speech run measures 1700.762 ms for the
surface world family, 1479.634 ms for capture, 54.170 ms for scene and 54.674 ms
for emissive. Decals, sky, effects, postprocess and UI add about 456 ms. The
`world geometry: pack` startup marker is emitted by the existing loader worker;
its interleaved timestamp does not establish a main-thread geometry stall.

The player now opts into staged preparation before admitting a loaded world.
The small UI pipeline is built first. One smaller auxiliary group is prepared
per frame; the four expensive world families run on one named worker using
cloned immutable device/layout handles, following the existing character shader
worker pattern. Ownership transfers through one channel. SDL/window operations,
surface configuration/acquisition, encoding, uploads and presentation stay on
the main thread. A format change discards a completed stale result before
starting another worker; jobs do not accumulate. Worker disconnection is an
error, and dropping its receiver on quit safely releases an undeliverable result.

A loading-only clear/UI/present path renders while preparation is pending.
Explicit developer captures can replay that UI without forcing synchronous
world compilation. World admission waits for complete pipelines, and the bar's
completed fill still follows actual loader milestones and reaches its final
value only after the GPU/game commit. The separate unknown-duration activity
marker does not contribute to completed progress. No timer or percentage
substitutes for work. The offline/compiler/editor path reuses the exact same
constructors synchronously; shader source, raster states and target formats are
unchanged.

## Correctness and production gates

Strict workspace/all-target/all-feature Clippy and the locked release workspace/
all-feature build pass. All 371 Python source tests pass with 18 expected skips;
674 assets validate with zero warnings. All 386 Rust workspace tests pass
(1.12 seconds test runtime; 12m25s fresh compilation). No new Rust tests or
GPU-dependent ordinary tests were added.

The native cohort verifies all six timed launches, loading pixels across resize,
final Full world rendering, retained NPC subtitles, quit while shader preparation
is pending, cancellation/repeated load, missing/corrupt packages and dismissal,
and editor Play/return. Every game trace ends in normal shutdown. Progress is
monotonic per request, and loading captures remain below completion. The main
world shader job completes once per ordinary process, including repeated loads.
The early-quit run exits normally before that job finishes. Native game/editor
processes use Normal priority; build/test/validation and editor compiler children
use BelowNormal. Small-fixture editor source JSON remains byte-for-byte unchanged.
Loading, world, subtitles, Explore return, errors and editor pixels were inspected.

The preserved environment package passes integrity validation with the new
compiler. The broad engine-source fingerprint changes automatically even though
baked-data semantics do not; `--require-current` therefore reports source-code
staleness. Its check was not weakened or frozen. Existing packages load normally,
and no shipped map was rebaked. Editor Play builds only its existing tiny fixture.
Future packaging must reconcile this conservative currency result explicitly.

## Remaining bounds and custody

Startup is not uniformly smooth. Candidate gaps still reach 0.23-0.36 seconds
around smaller atomic driver work and first surface presentation. More
significantly, first event pumping remains 8.454-8.502 seconds after entry
(baseline 8.338-8.801). This precedes the frame loop: `run_player` constructs
`LevelManager`, creates the device, then constructs a second manager/loader,
item state and audio before `FrameLoop::start`. The original traced spans are
about 2.884 seconds for the first catalog, 1.068 seconds for device/bootstrap
resources and 4.937 seconds from device setup to the responsive loop. The last
span is not separately attributed to each initializer. `AudioSystem::new` also
opens SDL audio and synchronously loads its cue data.

A bounded follow-up is a bootstrap state machine: retain main-thread SDL/device
operations, prepare catalog/model/cue data on owned workers, and pump/present an
initialization surface between completed units before constructing the full
frame loop. Audio initialization needs a separate decode/device boundary. This
pass does not claim that earlier blank-window/startup interval is solved.
Surface-format-change recovery is guarded in code but a physical format change
was not induced. Native minimization during shader compilation is unverified.

Mac validation remains explicitly deferred; no Mac host was accessed. The prior
OS mouse-capture limitation remains unverified and was not retried. Previous
UI evidence supplies broader sizes, navigation, long text, focus and conversation
coverage; it is not relabeled as new native input proof.

`local/verification-0.13.1-startup/` retains final executables/DLLs, exact hashes,
raw paired traces, native captures and assertions, gate logs, reproduction tools,
source snapshot/patch, currency receipts and cleanup/custody. `native-metrics.json`
contains all paired values. The earlier `verification-0.13.1-ui/` and attribution
roots remain unchanged. The only authorized remote write was the exact FPS report
commit `2f5b7d4c1a232e35a9624e48ec9994fd4b67a853` to `origin/0.13.1-dev`, verified
at that SHA. Its exact-SHA CI run `40846511547` passed; UI/startup commits remain local. No tag, release, upload or main merge.
