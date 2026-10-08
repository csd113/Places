# Stage 4 validation and handoff — 2026-10-08 UTC

[Native gallery](README.md), [lighting contracts](contracts.md),
[root-cause audit](root-cause-audit.json) and [measured costs](performance.md)
are the Stage 4 result. Publication and snapshot verification are appended once
their exact identities exist; repository custody is released only after final CI.
The [solver-15 final-audit correction](zero-source-repair.md) supersedes the
zero-selected-source limitation and appends its own validation/bundle publication.

## Entry and ownership

The existing `Art-style` checkout began clean at
`05c2e131c67f7d4d899ff181dc0958f83672a3c3`, the remote-verified Stage 3 seal.
Stage 3 handoff/contracts/measurements, Stage 1/2 diagnostics, deferred ledger,
canonical map/asset instructions and the established chronological journal were
read before implementation. Supported delegation selected GPT-6.1-sol/xhigh.
Workspace-write uses reviewed supported escalations for native/Git/network work;
no persistent permission, power or app setting changes implement “Full Access.”

| Owner | Disjoint deliverable |
| --- | --- |
| `compiler_probes` | Probe placement/validity, selected-direct means/moments, strict PLPF serialization, compiler provenance and Python decoder contracts; PLMP caster flag serialization under the agreed runtime claim interface. |
| `runtime_entities` | Spatial entity payloads/shaders/normals, stable direct accounting, cached moving triangle/alpha visibility, grounding and transform/source invalidation; actual character claim plan. |
| Primary | Cross-system interface and quality/resource integration, labelled segment visibility, bounded native controls/diagnostics, hero validation, measurements/evidence, Git and custody. |

Shared format/shader contracts were agreed before writes. The primary alone owns
index, build/test/native/bake jobs and publication. Specialists have finished and
are frozen; the late assembly support fix was returned only to its runtime owner.
No concurrent repository writer or measurement/build overlap remains. Only the
original hero and four additive hero controls are compiled/baked/rendered. Existing
workspace unit fixtures retain their usual scope. No nonhero bake, new dependency,
branch/worktree, main merge, tag, release or asset/concept edit is introduced.

## Implemented and accepted

PLPF v3 preserves combined incident HDR and aligns selected-direct coefficients
by stable source IDs. Runtime subtracts that direct before nonlinear reconstruction
and evaluates it once at transformed fragment positions/normals. Weak unselected
sources remain baked. Eight support anchors preserve spatial residual light;
labelled air plus straight segment checks reject cross-wall shortcuts. Missing
visible support is bounded and valid darkness remains zero. Selected switchable
fixtures reserve slots because their direct energy is absent from the base field.
PLMP v5 carries the actual character claim caster flag; partial/overflow static
fallback remains available. Legacy readers remain supported.

Runtime prepares one immutable static visibility resource and one model-local
triangle/PNG-alpha BVH per actual rigid mesh. Transform updates precede current
caster synchronization and receiver refresh; source/field/scene/transform/revision
identities invalidate caches. Self identity prevents self-blocking. Eight restrained
floor proxies use real bounds, source direction and authored floor, without a new
shadow draw/texture. Material, normals, HDR and tone mapping retain prior contracts.

The six matched original views and three real character views are inspected.
Prepared and runtime chairs retain coherent material and color response; grounding
and directional direct response improve modestly while the room/corner/hall keep
their accepted appearance. The same mesh through static, World and manual runtime
routes is accepted in warm, broad, panel-off residual, dim and night exterior views.
No generalized fullbright-in-dark, black-in-bright or outdoor-black defect was
reproduced in genuine original/control data. Black cat coat is authored reflectance;
white paws and unit-light diagnostics distinguish it from missing illumination.

A first candidate genuinely regressed the closed assembled door to black.
Extreme support points were inside separate frame stops. A generic min(1 cm,
2% axis) inset retains at least 96% per axis, clears those stops, and shares its
coordinates with shader interpolation. Real frame crossing remains occluded in
the assembly regression. Final native closed/open/closed images show leaf detail,
eight valid residual anchors and source visibility changing on a stationary chair;
both payloads restore exactly. Bidirectional aperture movement and rotations have
no major leak/pop/flicker at recorded positions. Original and failed evidence
remain immutable, with correction notes rather than rewritten history.

All six directed live preset pairs run twice in three lighting environments,
then twice again on the final actor binary: 48 transitions, no restart. Ten
independent advanced-setting changes exercise atlas Off/Medium/Full, filtering
Low/Medium/High and Low lighting disable/re-enable. High texture bytes remain
fixed during independent controls. Resident atlas/probe/uniform/source state is
valid; expected steady endpoints restore, with stationary advanced High pixels
identical. The original Stage 3 Low→High symptom was not reproduced. Code fixes
the audited requested/resident mutation and installs the complete captured request,
including reflections; later requests remain pending. Ready-only captures do not
claim every presented loading-transient frame.

Native translation, yaw and uniform scales 0.65/1.25/1.0 use ordinary object APIs.
Non-uniform scale is verified in deterministic matrix/support/normal contracts;
the public placement/runtime scale API is uniform, so an unsupported native
non-uniform authoring combination is explicitly untested. Probe, neighborhood,
direct/residual diagnostics use actual data. Final feature/normal pixels match.

## Limits and acceptance boundary

Supported hero cases meet the scoped acceptance criteria. No known essential
hero lighting or live-quality failure remains. This does not turn bounded
approximations into general simulation: eight support corners/clamped bind bounds,
64 admitted rigid casters, eight floor proxies and skinned bounds rather than posed
body rays remain. Static atlas direct and indirect remain baked during door movement; current ray
occlusion applies to entity receivers, while static floor grounding uses the
bounded footprint approximation. Entity
switchable diffuse bounce is absent from base probes. Genuine legacy v2 fields
retain center sampling; newly solved zero-selected-source fields retain v3 spatial
sampling and live switch accounting. These are disclosed
input/feature limits, not brightness compensation or hidden Stage 7 failures.

The global moving-caster revision conservatively refreshes every receiver. The
measured 32-receiver update is 0.699 ms median stationary, 7.559 ms moving,
including benchmark dispatch/logs and other engine work. GPU work is measured,
not inferred: original-view active work grows 0.740→1.046 ms; 32 stationary/moving
wide-view samples are 2.111/3.567 ms. No isolated direct/shadow shader timing or
physical display cadence is claimed. Bake/package/atlas/probe costs are in the
performance report. Original concepts/assets and previous snapshots hash-verify.

The only inherited local whole-workspace incompatibilities remain the three
stale package discovery failures. The partition-sensitive ghost-collider centroid
warning and nonhero final format/fingerprint migration remain Stage 7 ownership.
No quality failure is moved into that map ledger. Stage 5 presentation and Stage 6
measured caching/budget work are not started here.

## First frozen implementation local checks

All Rust commands use `RUSTC_WRAPPER=` because configured sccache is unavailable.
Final strict debug and release all-target/all-feature Clippy exit 0; format exits 0.
`cargo test --workspace` finishes **2097 library tests passed, 0 failed, 23 ignored**
in 485.24 seconds. Compiler CLI and command-line integration each pass one test.
It then exits 101 at the three unchanged `tests/list_levels.rs` cases, rejecting
inherited `home_showcase`, `geometry_intentional` and `level0_pit` dependencies.
Full local workspace green is not claimed. The remaining `macos_cpu_port` target
is run explicitly and passes. Exact clean-source CI supplies the complete all-feature
workspace gate independently.

```sh
RUSTC_WRAPPER= cargo fmt --all -- --check
RUSTC_WRAPPER= cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTC_WRAPPER= cargo clippy --release --workspace --all-targets --all-features -- -D warnings
RUSTC_WRAPPER= cargo test --workspace
RUSTC_WRAPPER= cargo test --workspace --test macos_cpu_port
```

Logs are individually named `fmt-final`, `clippy-debug-final`,
`clippy-release-support-final`, `tests-workspace-final` and
`tests-macos-cpu-port-final`. The 15 support regressions and seven Python decoder
tests pass. Earlier failed fixture setups remain recorded; they are corrected
without weakening assertions. The zero-test wrong filter is not counted as a pass.
Final `places-compile verify SOURCE --package PACKAGE --require-current` exits 0
for the original, entities, switchable, movement and comparison packages. The optional
probe audit produces byte-identical package bytes. No unchanged nonhero is rebaked.

## Reproduction and publication

Manifests beside every native run pin source/camera/FOV, 640×360 logical /1280×720
drawable, requested/effective settings, binary/package/source/catalog hashes and
capture commands. Original views keep FOV 60°, night sky/moon, no weather and
ready-world 0.5 seconds. Sequence captures explicitly record ready frames and
normal standing animation; they are not retouched. Final source/binary hashes
are in `build-provenance.json`; intermediate build inventories are retained.

The five final packages/binaries are retained outside `target`, under
`debug-maps/art-style-hero/evidence/`. Final immutable milestone snapshots
retain compatible player/diagnostic/compiler/SDL, real dependencies, catalog,
source/settings and launch instructions. Snapshot verification and implementation
links below pin the implementation commit. Stage 1 originals are
preserved permanently. The queue remains unfinished, so no `cargo clean` is run.

The implementation push and final documentation seal are separately remote-verified.
The final completion receipt reports the exact final SHA and successful CI URL;
a tracked report cannot include its own final commit hash. All workers and owned
native/build/test/trace processes must stop before explicit checkout/index/target
release. The inherited single `/usr/bin/caffeinate -di` PID 88945 retains queue
coverage and is transferred to parent `01a0fe65-4379-7018-adc6-f90754d9b0ca`.
Custody/completion metadata stays under `/tmp`, outside tracked deliverables.

## Verified implementation and runnable bundles

[Implementation 2615b9bc787a116bca1d9bc9d9deb855ed87724d](https://github.com/csd113/Places/commit/2615b9bc787a116bca1d9bc9d9deb855ed87724d)
is normally pushed to `origin/Art-style`; `git ls-remote` returns that exact SHA.
[Publication identity](publication.json) and [frozen source/bundle verification](snapshot-publication-verification.json)
confirm all 214 compiled source inputs match Git blobs and every retained file hash
matches. The journal seal changes documentation/evidence only. Exact final SHA/CI
result and ownership release are supplied in the out-of-tree completion receipt
after the final [Rust verification workflow](https://github.com/csd113/Places/actions/workflows/rust.yml?query=branch%3AArt-style) gate passes.

| Local snapshot under `debug-maps/art-style-hero/milestones/` | Files | Bytes | Native replay |
| --- | --- | --- | --- |
| [stage4 receipt](stage4-snapshot.json) | 41 | 193,587,814 | Six original views, all byte-identical to final accepted PNGs |
| [stage4-entities](stage4-entities-snapshot.json) | 42 | 194,828,647 | Three accepted actor environments; differences only within animated character bounds |
| [stage4-switchable](stage4-switchable-snapshot.json) | 42 | 198,172,450 | Ordinary broad-light actor view succeeds |
| [stage4-movement](stage4-movement-snapshot.json) | 45 | 195,384,094 | Door and exterior aperture cameras succeed; closed leaf is visually inspected |
| [stage4-comparison](stage4-comparison-snapshot.json) | 42 | 198,509,154 | Ordinary broad-light comparison view succeeds |

The [snapshot helper execution](snapshot-publication-execution.json) records actual
creation commands. Normal replay manifests in `snapshot-verification-*` record
actual commands/settings/native results; [main equality](snapshot-verification-main/equality.json)
and [actor comparison](snapshot-verification-entities/equality.json) qualify repeat
pixels. Skinned animation timing is not rewritten to force equality. Each control
has its own compatible package root because all deliberately retain hero ID
`art_style_hero`. Original Stage 1/2/3/control bundles remain hash-verified.

```sh
DYLD_LIBRARY_PATH="$PWD/debug-maps/art-style-hero/milestones/stage4" \
python3 tools/bench/capture_art_style_hero.py \
  --asset-root debug-maps/art-style-hero/milestones/stage4 \
  --manifest debug-maps/art-style-hero/milestones/stage4/hero-manifest.json \
  --binary debug-maps/art-style-hero/milestones/stage4/places --play --views entities
```

Use the same paths with `stage4-entities --views actors`,
`stage4-switchable --views actors`, `stage4-movement --views door` or
`stage4-comparison --views broad` for interactive controls. For bounded scripted
replay use `--out NEW_DIRECTORY --frames N --lighting-sequence NEW_SEQUENCE.json`;
copy the recorded sequence and change only its capture destinations to new paths.
Existing destinations are intentionally rejected. These are native macOS/Metal
bundles with preserved SDL, not cross-platform executables.

[Raw-log preservation](raw-log-preservation.json) retains original terminal bytes
outside target; tracked previews remove only repeated final LF bytes. No numeric
output, assertion or native PNG is edited. The final reviewed index contains only
task-owned source/control/docs and genuine immutable evidence.

## Solver-15 final publication

The [final-audit correction](zero-source-repair.md) fixes zero-selected-source
spatial presence and switch-only live entity accounting.
[Repair implementation 7d80433](https://github.com/csd113/Places/commit/7d80433ccb77d828a8c519ffa8da935fcbcac4c4)
adds focused authored-solver/label/codec/package/runtime regressions. Final local
checks are [recorded separately](repair-validation-v4/summary.json): strict
debug/release Clippy and format pass; 2,102 library tests pass, zero fail, 23 are
ignored. Workspace exit 101 remains only the three inherited Stage 7 package
discovery failures. The remaining macOS integration passes.
Intermediate exact-SHA [c6 CI](https://github.com/csd113/Places/actions/runs/37799238665)
passed, but the final repaired seal requires its own exact-SHA gate.

All five refreshed packages pass `verify --require-current`; all prepared render
payloads are byte-identical to their accepted predecessors. Only cache identities
and lightmap metadata descriptors change. New immutable local bundles are:

| Snapshot under `debug-maps/art-style-hero/milestones/` | Receipt | Verified native replay |
| --- | --- | --- |
| `stage4-v2` | [41 files](stage4-v2-snapshot.json) | Six original views, all byte-identical |
| `stage4-entities-v2` | [42 files](stage4-entities-v2-snapshot.json) | Actor view accepted; seven animated-body pixels differ |
| `stage4-switchable-v2` | [42 files](stage4-switchable-v2-snapshot.json) | Actor view accepted; five animated-body pixels differ |
| `stage4-movement-v2` | [45 files](stage4-movement-v2-snapshot.json) | Closed door view byte-identical |
| `stage4-comparison-v2` | [42 files](stage4-comparison-v2-snapshot.json) | Broad view accepted; six animated-body pixels differ |

[Bundle verification](repair-snapshot-verification.json) checks all 14 old/new
milestones and 215 source inputs. [Control launch commands](repair-control-replay-execution.json)
and replay manifests retain exact native provenance. Original Stage 1 baseline,
all preceding stages, original Stage 4 gallery and its measured GPU traces remain
immutable. No additional visual improvement or new runtime timing is claimed.

From the repository root, launch the final hero with preserved assets/settings:

```sh
DYLD_LIBRARY_PATH=/Users/connordawkins/Documents/GitHub/Places/debug-maps/art-style-hero/milestones/stage4-v2 \
python3 tools/bench/capture_art_style_hero.py \
  --asset-root debug-maps/art-style-hero/milestones/stage4-v2 \
  --manifest debug-maps/art-style-hero/milestones/stage4-v2/hero-manifest.json \
  --binary debug-maps/art-style-hero/milestones/stage4-v2/places --play --views entities
```

For a reproducible six-view replay, replace `--play --views entities` with
`--out NEW_OUTPUT_DIRECTORY`. The other receipt launch/replay commands select
their compatible package and real dependencies. Binaries and SDL are macOS native.

No essential supported hero or live-quality acceptance failure remains after the
repair. Previously disclosed bounds/contact/skinned/static-atlas/switch-bounce and
loading-transient limits remain. The measured conservative caster invalidation
cost is a documented Stage 6 profiling lead. Stage 5 and nonhero migration are not
started; `target/` is retained for the unfinished serial queue.

The final publication SHA, remote verification, exact-SHA CI and explicit owner
release are in `/tmp/places-art-style-stage4-completion.json` and durable
`debug-maps/art-style-hero/evidence/stage4-completion.json`, written only after
the gate. The queue's existing `caffeinate -di` PID 88945 remains alive for transfer
to the coordinator; no duplicate or persistent setting change is introduced.
