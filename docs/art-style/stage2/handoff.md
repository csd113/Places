# Stage 2 validation and handoff — 2026-10-08 UTC

[Visible before/after result](README.md), [contracts](contracts.md),
[performance/resource evidence](performance.md) and the existing
[chronological index](../../style-upgrade-20261007/README.md) accompany this stage.
Stage 1 remains immutable. No Stage 3 work is started.

## Ownership and scope

Canonical checkout `/Users/connordawkins/Documents/GitHub/Places`, existing
`Art-style`, initial clean local/origin `94d22f04a83156a3fe8ad9d16ef6d22732dc9818`.
Sole Stage 2 checkout/index/target ownership; no subagents, branches or worktrees.
Inherited model GPT-6.1-sol / xhigh was supplied by the supported delegation.
This environment uses workspace-write with reviewed escalations; persistent Full
Access is not independently claimed changed. Native Metal/process/GitHub reads
use those supported controls. No persistent power settings change.

Only `tests/fixtures/levels/art_style_hero.json` is compiled/baked or visually
accepted. Source, catalog, model/texture/concept files, lights and camera manifest
are unchanged. Existing unrelated local sources/packages remain untouched.

## Exact final validation

`RUSTC_WRAPPER=` disables this host's unavailable sccache wrapper. Commands and
exit codes are in [validation-execution.json](validation-execution.json).

- `cargo fmt --all -- --check`: exit0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit0.
- `cargo clippy --release --workspace --all-targets --all-features -- -D warnings`: exit0.
- `cargo test --workspace`: **exit101**. All **2,042 library tests pass**, zero
  library failures, 23 ignored; two preceding integration tests pass. The same
  three inherited `tests/list_levels.rs` failures remain:
  `another_working_directory_lists_the_same_packages`,
  `packaged_layout_lists_only_packages`, `repository_layout_lists_only_packages`.
  Local `levels/home_showcase.placesmap`, `geometry_intentional.placesmap` and
  `level0_pit.placesmap` have prior asset-byte dependency mismatches. Exact logs
  and Stage 7 ledger retain the failures; later integration binaries are not
  reached by this local run. No assertion, fixture or discovery filter is weakened.
- Feature shader/renderer tests: **204 pass, 10 ignored**, zero failures;
  [log](test-diagnostics-final.log). Subsequent focused legacy controls pass and
  final workspace covers the final counter fix. Tests cover conversion/alpha mips,
  float/HDR preservation, inverse transpose/posed normals, UV handedness, model
  material defaults and alpha/response records, filtering and target formats.
- Native Metal `cargo test --lib --features visual-diagnostics round_trip --
  --ignored --nocapture`: **2 pass**. sRGB decode/encode has zero byte error for
  all256 input values; HDR cube upload/render/sample orientation agrees with the
  reference face convention. [Native log](test-native-gpu.log).
- `cargo test --lib --all-features legacy`: **4 pass**, including old prop3 and
  probe2 compatibility readers. [Log](test-legacy-readers.log).
- `python3 tests/test_glb_materials.py`: **4 pass**, no generated asset files.
  `python3 -m py_compile` of changed four Python modules succeeds.
- Release normal player/compiler and opt-in diagnostic player build successfully.
  Preserved outside target, along with all matching dependencies and SDL runtime.
- Hero forced compile: exit0, 12 workers, all Off/Medium/Full variants. Two builds
  reproduce package SHA256
  `b38a022a2762251ab49e475de494791ecf1cae24a30060e0be90edacc85aa1cd`.
  [Build receipt](hero-build.json), [OS cost receipt](hero-build.log),
  [inspect](package-inspect.json), [require-current integrity check](hero-verify-sealed.json).
- Native six-view before/final captures, independent repeats, material/normal/
  lighting diagnostics and independent quality controls: exit0 and inspected.
  [Capture equality](repeatability.json), [diagnostic commands](diagnostic-execution.json),
  [quality-control commands](control-execution.json),
  [final capture/runtime commands](native-final-execution.json) and
  [final diagnostic commands](diagnostic-sealed-execution.json). Genuine baseline and intermediate
  images remain unchanged. No moving/posed character or non-uniform JSON-placement
  capture is claimed; supported matrix/skin behavior is deterministically tested.

Earlier failed development runs are retained honestly under their original log
names, including `test-lib-final.log` (a failed intermediate despite its early
filename). The final authoritative workspace log is `test-workspace-sealed.log`.
The initial 11.44 s bake overlapped a test run and is excluded from cost comparisons.

## Measured costs

The forced hero build is7.44s /660.7MiB peak, versus4.89s /659.1MiB in Stage1.
The first HDR reflection capture phase accounts for the wall-time rise. Package
size rises16.72% to4,521,326 bytes; atlas size/chart count and source triangles
are unchanged. High colour-target allocations rise7,833,600 bytes; aggregate VBO
rises18.75%. High room median CPU submission7.098→4.069ms /peak RSS433.3→430.0MiB
is an observed short desktop sample, not a measured GPU speedup. All six bounded
samples submit360/360 frames. Exact per-view distributions, allocation arithmetic
and exclusions are in [performance.md](performance.md).

## Acceptance and next-stage boundaries

Deliberate linear/HDR boundaries; compatible defaults/readers; consistent supported
normal/material/alpha paths; real colour/data semantics; correct colour mip energy
and independent size/filter controls are accepted. The six native High views show
clearer pale surfaces without broad material regression or accidental double gamma.
Light, source art and exposure are not adjusted to fit screenshots.

Stage 3 owns sofa receiver/chart seams, GI distribution and transmission. Stage 4
owns bounds-centre probe spatial coverage, contact and moving/posed illumination.
Stage 5 owns exposure/shoulder/bloom/fog/AA, blend emission coverage and family
ordering. Fine plastic noise remains existing artwork/detail, not remodeled here.
Stage 7 owns all non-hero prepared migration and deliberate GLB scalar adoption.
GPU execution time and cross-platform native rendering remain unmeasured. Full
local workspace green is still blocked by the three preserved Stage 7 packages;
clean-source exact-commit CI is a separate publication gate.

## Publication and custody

[Implementation b34e1dd6d8dcdf43280a35c9644bcb567905e492](https://github.com/csd113/Places/commit/b34e1dd6d8dcdf43280a35c9644bcb567905e492)
is committed and pushed to `https://github.com/csd113/Places`, `origin/Art-style`.
`git ls-remote origin refs/heads/Art-style` returned that exact implementation SHA.
The follow-up journal/snapshot seal changes documentation and evidence only.
[Branch verification runs](https://github.com/csd113/Places/actions/workflows/rust.yml?query=branch%3AArt-style)
run format, locked all-target/all-feature check, strict debug/release Clippy,
full all-feature workspace tests and whitespace against the clean CI checkout.
The exact final remote SHA, CI run URL/result and release receipt are supplied in
the coordinator completion message after that gate succeeds; a committed report
cannot contain its own final hash. No repository release is claimed before that
exact-commit CI gate. The initial implementation run can be superseded by the
final documentation push through the existing workflow concurrency policy.

[Stage 1](stage1-snapshot.json) and [Stage 2](stage2-snapshot.json) snapshots each
preserve41 verified, read-only files outside target. Stage 2 pins the implementation
revision above; both native room replays match their respective galleries exactly.
Stage 2 bundle size192,471,565 bytes. `milestone.json` indexes every original and
intermediate six-view gallery for the eventual seven-stage comparison. Inherited task-owned `/usr/bin/caffeinate -di` PID88945 remains
active with indefinite `PreventUserIdleSystemSleep` and
`PreventUserIdleDisplaySleep` assertions. Its custody is recorded outside tracked
deliverables in `/tmp/places-art-style-queue-custody.json`; it must remain alive
through the queue. On successful final CI, the completion handoff explicitly releases checkout/index/
target and transfers inhibitor custody back to coordinator thread
`01a0fe65-4379-7018-adc6-f90754d9b0ca`, without starting Stage 3 or cleaning target.
All native players, builds, bakes and tests are joined before release; there were
no subagents. Target's most recent player build has the optional diagnostic
feature; the preserved normal player/compiler are the ordinary launch artifacts.

```sh
DYLD_LIBRARY_PATH="$PWD/debug-maps/art-style-hero/milestones/stage2" \
python3 tools/bench/capture_art_style_hero.py \
  --asset-root debug-maps/art-style-hero/milestones/stage2 \
  --manifest debug-maps/art-style-hero/milestones/stage2/hero-manifest.json \
  --binary debug-maps/art-style-hero/milestones/stage2/places --play --views room
```

The parent checkpoint connector returned `thread not found`; ordinary task
progress and this final completion receipt are the available communication path.
That connection issue does not stop independent authorized Stage 2 work.
