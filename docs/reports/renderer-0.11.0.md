# Renderer foundations — 0.11.0 staging

The targeted implementation, independent source review and native acceptance
campaign are complete on Apple M2 Pro/Metal. This report records mixed throughput
results, bounded resource observations, the structural Office repair and explicit
limits. Final staging CI is checked at the exact pushed SHA before issue #16 is
closed; completion evidence is linked from that issue.

## Implemented corrections

The evidence supported targeted changes rather than a renderer rewrite.

- Retire pending surface images, saved capture frame and HUD replay whenever
  drawable/world resources change or the device becomes fatal. Previously the
  saved frame was never invalidated despite its documented lifetime contract.
- After one Outdated acquisition retry, retain Lost recreation and Validation
  fatal handling. Recreated surfaces refresh colour, alpha and present modes;
  incompatible formats fail before configuration. The retained SDL window clone
  keeps raw handles alive through surface destruction.
- Replace mutable renderer scene state on explicit same-ID reloads, including
  authored presentation, door sources, runtime models, visibility, weather and
  effects. Graphics transitions inherit playback only when explicitly requested.
  Stop staging, pose/environment, material and readback work after device loss.
- Write changed runtime emission scales into existing material uniforms instead
  of recreating all dynamic geometry/materials. Stable object identities protect
  removed/reused slots; unchanged values do not write again.
- Cache dynamic/character UV visibility anchors using exact source descriptors,
  receiver transform/bounds/identity, immutable transport identity and movable
  occluder revision. Installation replaces the owning GPU cache. Borrow water
  volumes during floating motion and reuse each computed dynamic transform.
- Classify immutable world passes once on upload. Gather translucent families
  into one stable global sort and visit a selected prop/dynamic entry directly.
  Shader bytes, depth precision/conversion, rasterization, material alpha,
  shadow sampling, water and lighting quality were not changed.

`visual-diagnostics` enables wgpu counters. Opt-in resource JSON records live
HAL objects and observed peaks at lifecycle boundaries/every sixty frames, plus
partial texture/atlas/post texel budgets. Metal does not implement HAL allocation
byte counters: those fields are null, not zero. Cache references can share GPU
allocations and must not be summed as total VRAM. The native launcher now records
actual app RSS samples at one-second intervals, excluding wrapper memory.

## Offline geometry foundations and repair

The architecture checker now uses conservative spatial candidates with a
2,000,000-visit work cap instead of quantized plane bins/quadratic bucket scans.
An exhausted budget reports `geometry-check-incomplete`. Local plane checks use
all triangle corners. Tiny regressions cover remote origins, old normal-bin
boundaries and counted active-pair work.

The shared Python triangle kernel retains stable node/mesh/primitive/triangle
addresses, full vertex records, material/alpha information and explicit instance
matrices. It reports exact, near-coplanar and opposite-facing coverage. Every
visited pair consumes the explicit budget; incomplete audits have no repair plan.
Generic deletion requires exact equally oriented OPAQUE faces with identical
records/material in the same source primitive. Near planes, different materials,
seams, opposite sheets and placement clashes stay review-only. Skins, animation,
morph targets/default weights require pose review. Mutators reject unknown,
additional or trailing GLB chunks before editing and reverify hashes, records and
an empty second duplicate plan.

`--prop-instances` exports the engine's actual floor/yaw/scale/span/shear matrices.
`--audit-instances` records source/model identities and considers distinct fixed
GLB instances. Unresolved material override alpha is `UNKNOWN_OVERRIDE`.
Architecture/prop contacts, fixtures, doors and live poses are outside this
static-instance report. No per-frame pair scan or editor UI was added.

Depth review verified the native `Depth32Float` target, 0.1–100 m scene clip
range, the shared OpenGL projection/frustum convention and its single wgpu
`[-1,1]`→`[0,1]` depth correction. Ideal float-bin spacing is about 0.2 μm at the
0.575 m desk close-up, 0.6 μm at 1 m and 60 μm at 10 m; these analytic values
exclude hardware interpolation error. The diagnosed two opaque materials occupy
the same plane, with no intended depth separation. More depth precision would
not establish their intended ownership. Existing explicit decal layering/bias
remains separate from opaque model repairs; no global bias or clip-plane change
was added. Authored layered ribbons, paper backs and mounting contacts remain
review findings, not automatic deletion targets.

The Office `top_reception` desk had 18 confirmed same-facing wood/paint perimeter
triangle overlaps. The explicit reviewed recipe retains the existing opaque
paint band and clips only covered wood at its top plane. It moves no surface.
The GLB changes from 256 to 262 triangles; bounds, material dictionaries, four
embedded PNG payloads and other primitives remain exact. The repaired SHA-256 is
`f8a4f4198dcebabfb25519d96acdbcf8a6d352096381e7e85c032b5ffe6351cc`.
The targeted overlaps fall from 18 to zero even at a 1e-8 m² report threshold.
Five tiny foot/internal same-facing contacts remain review findings. Original
bytes and the hash-bound inspectable plan are preserved outside `target`.
A single fresh Office package build is used for candidate validation. Its
validated bytes are installed into the ignored `assets/levels/office_demo.placesmap`
for normal local play and pass `verify --require-current`. The exact original
compiled package was not retained: the baseline levels directory was a symlink
to live packages when the replacement was installed. The original authored desk
GLB, before captures, original package hash and byte-comparison receipts remain
preserved. The owned baseline overlay is now isolated and omits Office rather
than presenting replacement bytes as the original; unchanged maps remain linked.

Current Arcade/Office model audit: 87 static assets completed, five pose-dependent
assets excluded. Most complete at two million visits; the Office snack machine
needed an explicit twenty-million cap (12,174,410 visits, 18.113 seconds). No
unreviewed generic deletion was applied. The Office transformed-instance audit
completed at 11,965,190 visits with 35 opposite-facing contacts/five exclusions.
Arcade remains **incomplete at 100,000,000 visits** (45.263 seconds), with eight
observed opposite-facing paper-holder/divider mounting contacts and 32 live/pose
exclusions. Its partial results do not establish a clean map. All 34 existing
base-placement duplicate warnings are intentional balloon bouquets with distinct
rig offsets, sizes, colours and phases; no placement was removed.

## Native performance comparison

Hardware: Apple M2 Pro/Metal, Retina 2×, High textures/filtering, Full lightmaps
and reflections, bloom on, FOV 60. Both executables use the prerequisite Rust
1.99.1/dependency baseline. The before executable is from `e29adae`; the after
executable contains the reviewed `5d9983e` implementation. Actual presentation
switches to Immediate, settings say VSync off, and each BENCH_SUMMARY records
swap_interval 0. No engine FPS cap is active.

Each side has three separate runs per view, 120 ready-frame warm-up and 600
measured frames at a fixed 1/60 simulation step. FPS uses real wall time:
`total measured frames / sum(loop_ms) × 1000`. The table pools 1,800 frames per
side/view. All twelve paired runs match settings, camera, physical drawable,
package hash, movement script, fixed step and per-frame draw calls, visible
indices and reflection-pass counts; all report 720 ready presentations and zero
missed-ready frames. CPU/GPU diagnostic trace flags are absent on both sides.
No owned build/bake ran during timing. Separate 3,600-frame preconditioning runs
completed on each executable; permission waits and their lost-wrapper recovery
are excluded from comparisons. Every comparison still performs its own identical
120-frame warm-up. Thermal checks before/after report no recorded thermal or
performance warning; no CPU power status is available. These are sequential
batches, not interleaved trials or temperature-controlled measurements.

| View / drawable | Before average FPS | After average FPS | Change |
| --- | ---: | ---: | ---: |
| Bathroom lateral, 1512×838 | 378.63 | 470.95 | +24.38% |
| Bathroom lateral, 3024×1676 | 217.68 | 216.94 | -0.34% |
| Close closed door, 3024×1898 fullscreen | 130.43 | 128.44 | -1.53% |
| Arcade cabinets, 3024×1898 fullscreen | 227.55 | 227.73 | +0.08% |

The small-resolution lateral result is an observed 24.38% throughput increase;
its per-run before/after ranges are 371.59–390.24 and 463.04–480.21 FPS. The
larger lateral and cabinet views are close in throughput. The close-door view is
**1.53% slower**, consistently across its three runs; this is reported as a small
tradeoff, not erased as “no regression.” There is no evidence of a shader speedup.
A useful maintenance guardrail from this hardware baseline is to investigate
losses greater than 5% of average FPS under these same conditions; it is explicit
developer tooling, not a full-map CI requirement. This campaign's maximum
measured decrease is below that guardrail.

| View | Update mean ms, before→after | Render mean ms, before→after | GPU elapsed ms (median of run medians), before→after |
| --- | ---: | ---: | ---: |
| Bathroom lateral, 1512×838 | 0.902→0.872 | 1.576→1.086 | 1.891→1.885 |
| Bathroom lateral, 3024×1676 | 1.017→1.026 | 3.412→3.417 | 5.018→5.023 |
| Close closed door, 3024×1898 fullscreen | 1.233→1.265 | 6.233→6.299 | 8.091→8.099 |
| Arcade cabinets, 3024×1898 fullscreen | 1.120→1.118 | 3.082→3.093 | 4.807→4.806 |

Render time includes encoding, submission and waiting, rather than pure CPU
execution. GPU elapsed is the union of pass-stage spans, including scheduling
stalls; overlapping pass spans must not be added as independent active costs.
The low-resolution improvement with nearly unchanged GPU elapsed is consistent
with reduced CPU-side submission work. Source changes demonstrably remove
repeated classification, sorting, scanning, UV-anchor construction and resource
recreation; this sequential experiment does not isolate each contribution.
Close-door GPU elapsed remains about 8.1 ms at 233 base scene draws versus
about 4.8 ms at 98 cabinet-view draws. This comparison combines geometry and
fragment work; it does not isolate their costs. Changing the lateral drawable
from 3024×1676 to 1512×838 reduces GPU
elapsed from about 5.0 to 1.9 ms. Lighting/material work and covered pixels remain
relevant, and no effect or scene content was reduced to improve the result.

The historical near-20-FPS case was not reproduced. Earlier
[doorway](performance-0.10.0.md) and [multicore](multicore-0.10.0.md) reports already
document fragment-visibility/lighting fixes included in this baseline; their
gains are not attributed to this stage. The early 119.59→217.66 FPS / +82%
lateral comparison is **superseded**: repeated before samples also reach 217.68
FPS. Earlier instrumented cabinet timing and incorrect wrapper-RSS receipts are
retained as diagnostic history and excluded from the accepted table.

Per-run detailed distributions, sampled app RSS, load state, binary/package
hashes and native presentation identity remain in receipts. The largest sampled
single-view RSS peak was 1,428.3 MiB before and 1,053.4 MiB after; sampling and
transient allocations vary, so these maxima are not a claimed memory saving.
Longer map-transition memory evidence is separate below.

Genuine matched door, Bowling Alley and Pool basin captures are PNG-byte and
pixel identical to baseline at 1512×838, with matching package/settings, camera,
draw counts and triangles. The fresh Office close-up has a continuous counter
edge and no visible holes or material discontinuities. Its 8,930 changed pixels
(0.705%) differ by at most two RGB levels; visible triangles increase by six as
expected. Structural overlap removal is established offline; a static capture
alone does not prove temporal flicker removal. Office package validation and
current-source verification passed; collision, navigation and semantics are
byte-identical to the original package.

The Pool control activates two bodies. The initial nearby trace contains 1,822
physics frames, 254 held frames, finite pose/render-pose components and no
recovery, but its camera obscured the duck. A corrected dry-deck run removed
`PLACES_CAMERA` (which deliberately pins render angles despite scripted aim).
Its genuine free/held/thrown/moving images clearly show the duck on the deck,
close to the player while held, then farther away after throwing, with model
lighting and material faces intact. The 1,601-frame trace has 296 held frames,
finite transforms and zero recovery. Group reset leaves two dormant bodies;
reactivation resumes them. This is reset/reactivation evidence, not body-removal
proof. Map transitions separately replace the Pool world and its resources.
Transparent water, caustics, opaque rails/deck and the moving duck were inspected.
The empty-asset native launch renders the expected no-map menu and exits normally.

## Native resource and transition acceptance

The diagnostic executable completed 40,000 measured frames and exited normally.
Window creation through shutdown took 261.24 seconds; its 777.54-second launcher
elapsed time includes a 516.05-second Documents approval wait and is not a
performance measurement. Scripted transitions cover Arcade quality low/high,
lighting/static-to-full changes, Arcade↔Pool twice, explicit same-ID Arcade
reload, Office, return to Arcade, and 1280×720→1512×838 drawable changes.
The lighting action labelled `static` selects the implementation's lower-lighting
mode, disabling lightmaps/reflections, before `full` restores them. Both real SDL
minimize/restore calls were issued at configured frames with no warning, and
rendering continued. The harness does not emit an observed OS minimized-state
receipt, so that state is not independently proven. Resize/drawable transitions
and fresh in-engine captures are recorded. True physical device loss remains
policy-tested rather than induced.

There are 974 sampled resource records and no fatal state. At the final 185 frame
samples (frames 30,000–41,040), textures remain 142, bind groups 1,311, render
pipelines 56, samplers 22 and the persistent cache 116 entries. Buffers vary
677–682 and texture views 158–160 during settling. Transition
observed peaks of 8,587 buffers and 179 textures return to those settled ranges;
pending upload/staging state clears. Counts do not show accumulating map resources
in this campaign. Sampled actual-app RSS peaks at 2,487.7 MiB during transitions
then returns to 893.2–897.1 MiB in the final minute (median 894.5 MiB, last
895.2 MiB). That bounded observation is not proof of zero long-term growth.
Metal allocation bytes remain unavailable/null, sampled peaks are not exact
high-water marks, and partial texture/atlas/post budgets cannot be summed.

The existing default, diagnostics, baseline and final-candidate launches continued
after their Documents prompts resolved. When executor reconnections lost a
wrapper, completed preconditioning output was retained, native process exit was
checked, and only unstarted timing commands were resumed. The stable
`io.github.csd113.places.validation` identity was used throughout. No permission
was reset, identity spoofed, access broadened, denial bypassed or pending launch
duplicated. No native or build job remains after acceptance.

## Verification and limits

| Gate | Result |
| --- | --- |
| Prerequisite exact-SHA CI 38857268227 at e29adae | Success |
| Format check | Passed |
| Strict workspace/all-target/all-feature Clippy | Passed, debug and release |
| `cargo test --workspace` | 2,340 library + 7 integration passed; 19 existing ignores |
| Locked all-feature workspace tests | 2,347 library + 7 integration passed; 19 existing ignores |
| Final source Python suite | 372 run, passed; four optional skips |
| Final focused offline geometry suite | Nine passed in ~0.005 s |
| Asset validation | 593 assets / 370 placeable / 8 themes; zero warnings |
| Existing explicit native headless GPU resource controls | Both passed (quality toggles and entity map reloads); no production map bake |
| Release tools + diagnostics build | Passed; binaries preserved outside target |
| Independent complete source/asset/test review | No unresolved source findings |
| Default candidate captures, no-map menu, matched timing | All 24 comparison runs exited 0 and matched per-frame geometry |
| Diagnostics native transition/resource memory | 40,000 measured frames, 261.24 native seconds, exit 0; bounded settled census/RSS observed |
| Visible moving/held/thrown prop and reset/reactivation | Genuine captures and finite pose trace inspected independently |

All existing Rust tests remain. Four new owning-subsystem CPU tests cover UV
cache invalidation, retry policy, replacement capabilities and bounded geometric
candidates; their fixtures are scalar lists/two or three triangles, with no GPU,
production asset or map bake dependency. Nine Python cases protect exact repair,
alpha/material/UV distinctions, transforms, conservative candidates, budgets,
morph declarations, chunk safety and affine clipping. Existing default tests were
not expanded into full-map or performance campaigns.

Physical device loss was not induced, OS minimized-state receipt is unavailable,
and memory observations cover this bounded campaign. Linux/Windows physical-GPU
native execution is unverified. Ambiguous overlaps and posed geometry require review;
the broader editor workflow stays in 0.12.0. Audio, test removal, a water rewrite
and the dedicated flashlight close-up investigation (#23) are outside this stage.
No flashlight shader/shadow behavior was changed. Its retained 1024² depth map,
four PCF taps and material alpha coverage were audited; projected bounds shadows
still have an eight-subject budget and an approximate silhouette. These are
relevant context for the queued close-up fake-shadow investigation (#23), not
claims that investigation is complete.

Evidence, original/repaired desk plans, audit receipts, command logs, captures,
source hashes and runnable binaries are under `local/renderer-0.11.0/`, outside
`target`. Default candidate player SHA-256:
`7b24a1e00f3107350530115455c9e800bb180da5dcd08aa827415b285e94c92b`.
