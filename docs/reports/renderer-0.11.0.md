# Renderer foundations — 0.11.0 staging

Implementation and independent source review are complete. Native candidate
acceptance is pending a macOS Documents-folder approval for the stable Places
Validation app. Issue #16 remains open; no candidate performance improvement or
native visual/resource acceptance is claimed while that launch is blocked.

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

The Office `top_reception` desk had 18 confirmed same-facing wood/paint perimeter
triangle overlaps. The explicit reviewed recipe retains the existing opaque
paint band and clips only covered wood at its top plane. It moves no surface.
The GLB changes from 256 to 262 triangles; bounds, material dictionaries, four
embedded PNG payloads and other primitives remain exact. The repaired SHA-256 is
`f8a4f4198dcebabfb25519d96acdbcf8a6d352096381e7e85c032b5ffe6351cc`.
The targeted overlaps fall from 18 to zero even at a 1e-8 m² report threshold.
Five tiny foot/internal same-facing contacts remain review findings. Original
bytes and the hash-bound inspectable plan are preserved outside `target`.
A single fresh Office package build is used for candidate validation.

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

## Native baseline and performance status

Hardware: Apple M2 Pro/Metal, Retina 2×, High textures/filtering, Full lightmaps
and reflections, bloom on, FOV 60. VSync is off, actual presentation switches to
Immediate, and BENCH_SUMMARY records swap_interval 0. The game has no additional
FPS cap. Each timing sample discards 120 ready frames and measures 600 at a
fixed 1/60 simulation step; timings use real wall time. No owned build/bake ran
during these baseline timing samples. Captures/CPU traces are separate evidence.

Average FPS is measured frame count divided by total loop time. These bounded
baseline samples are not a before/after performance claim:

| View / drawable | Baseline average FPS | Candidate average FPS | Change |
| --- | ---: | ---: | ---: |
| Bathroom lateral, 1512×838 | 119.63 | Pending | Unavailable |
| Bathroom lateral, 3024×1676 | 119.59 | Pending | Unavailable |
| Close closed door, 3024×1898 fullscreen | 129.36 | Pending | Unavailable |
| Arcade cabinets, 3024×1898 fullscreen | 226.70 | Pending | Unavailable |

The historical near-20-FPS case was not reproduced on this prerequisite build.
The earlier [doorway](performance-0.10.0.md) and
[multicore](multicore-0.10.0.md) reports already document shader fixes included
in this baseline; those gains are not attributed to this work. Current GPU
elapsed medians were 8.107 ms close-door and 4.830 ms cabinets. Door CPU update
mean was 1.012 ms; the separate cabinet diagnostic measured character skin/upload
spans about 0.65–0.87 ms. Metal pass spans overlap and cannot be added as
independent costs. Sampled close-door/cabinet process RSS peaks were 929/963 MiB.
Earlier two lateral receipts predate corrected launcher RSS sampling; their
wrapper RSS values are explicitly excluded.

Genuine baseline in-engine captures of the door, Pool basin/water, Bowling Alley
and Office desk were inspected. Static images identify the target geometry but
do not establish temporal shimmer removal or moving-object correctness. The
candidate has produced no frame, capture or GPU identity yet: macOS TCC reports
`AUTHREQ_PROMPTING` for `kTCCServiceSystemPolicyDocumentsFolder` after the new
binary failed the prior code requirement. A one-second sample confirms its main
thread is blocked in file open before window creation. This is a pending host
permission, not a renderer performance measurement or denial. No permission was
reset, app identity spoofed, security access broadened or denial bypassed.

## Verification and remaining work

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
| Candidate native visuals/performance/transition memory | Pending host Documents approval |

All existing Rust tests remain. Four new owning-subsystem CPU tests cover UV
cache invalidation, retry policy, replacement capabilities and bounded geometric
candidates; their fixtures are scalar lists/two or three triangles, with no GPU,
production asset or map bake dependency. Nine Python cases protect exact repair,
alpha/material/UV distinctions, transforms, conservative candidates, budgets,
morph declarations, chunk safety and affine clipping. Existing default tests were
not expanded into full-map or performance campaigns.

Pending candidate acceptance includes matched visual comparisons, average FPS
before/after and percentage changes, moving/held/spawn/remove physics transforms,
lighting/shadows/transparency, repeated map/settings/resize/minimize transitions,
settled resource/RSS observations and an extended native session. True device
loss is policy-tested, not physically induced. Linux/Windows physical-GPU native
execution is unverified. Ambiguous overlaps and posed geometry require review;
the broader editor workflow stays in 0.12.0. Audio, test removal, a water rewrite
and the dedicated flashlight close-up investigation (#23) are outside this stage.
No flashlight shader/shadow behavior was changed.

Evidence, original/repaired desk plans, audit receipts, command logs, captures,
source hashes and runnable binaries are under `local/renderer-0.11.0/`, outside
`target`. Default candidate player SHA-256:
`7b24a1e00f3107350530115455c9e800bb180da5dcd08aa827415b285e94c92b`.
