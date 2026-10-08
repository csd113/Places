# Stage 1 handoff — 2026-10-08 UTC

Stage 1 establishes a native hero benchmark, grounded gap audit and development
diagnostics. It does not implement or claim a lighting/color/material improvement.
Stage 2 has not started. This receipt is active until the final pushed revision
and custody checks below are recorded.

## Delivered

- [Hero source](../../tests/fixtures/levels/art_style_hero.json): furnished room,
  connected hallway, glazed exterior, broad fluorescent/warm practical, completed
  material families, static props and an ordinary spawned comparison chair.
- [Original six-camera/settings manifest](hero-manifest.json),
  [additional table camera](hero-diagnostics-manifest.json) and
  [native launcher](../../tools/bench/capture_art_style_hero.py).
- Six original High views, two Low controls, three matched repeats and three
  normal after-build captures. The normal after-build and feature-final comparisons
  are byte-identical. This is preservation/repeatability, not a visible gain.
- [Eleven selectable native diagnostic modes](diagnostics.md), capture-time
  requested/applied/resident state and real offscreen scene counts. Normal release
  excludes the shader selector. No authored edits/rebakes are required to select.
- Saved-package export and one scoped live diagnostic hero solve: real direct,
  diffuse indirect, filtered/final, receiver positions/normals/material inputs,
  true charts and exact caster ownership/rays. Package bytes remain identical.
  Missing runtime direct/indirect/shadow/AO/metallic/probe views are labeled
  unavailable. Stage 4 has the probe-view extension seam.
- [25-case/37-image raw native campaign](diagnostics/campaign-execution.json),
  [state checks](diagnostics/native-state-summary.json) and
  [six byte-identical feature/live control pairs](diagnostics/control-comparisons.json).
  Independent atlas/filtering/Low-lighting controls and genuine live endpoint
  transitions preserve the chair. Loading transients remain qualified.
- [Audit and strengths](README.md), [Stages 2–7 dependencies](gap-plan.md),
  [costs](performance.md), [exact ledger](deferred-ledger.json),
  [validation](validation.md) and the dated entries in the
  [existing chronological journal](../style-upgrade-20261007/README.md).

Normal interactive launch, from repository root after the documented explicit
hero compile:

```sh
python3 tools/bench/capture_art_style_hero.py --play \
  --binary debug-maps/art-style-hero/evidence/places-normal --views room
```

The final normal release is also restored in `target/release/`. Verified normal,
diagnostic and original baseline executables, package and raw lighting exports
are preserved in ignored `debug-maps/art-style-hero/evidence/` outside `target/`.
A fresh checkout reproduces them via [normal](README.md) and
[diagnostic](diagnostics.md) commands; output destinations must be new.

## Acceptance, costs and limitations

Normal/feature release builds, strict Clippy, hero integrity/currency, all-variant
bake, native capture/receipt checks, chart audit, source/reference hashes,
asset contracts, seven Python tests and thirteen focused Rust diagnostic tests
pass. Geometry has zero errors and one retained decorative open-garden warning.
Physical normal composition/package bytes, concepts, prior assets, original hero
and production world WGSL remain unchanged. Final built-source hashes agree.

`RUSTC_WRAPPER= cargo test --workspace` exits 101: **2,029 library tests pass,
23 ignored; the same three level-discovery tests fail** on inherited stale local
packages. Exact dependencies/current checks and Stage 7 resolution are in the
ledger. They were not removed, hidden or repaired outside hero scope. No new
source-test failure was observed. Clean-source baseline CI passed;
[diagnostic implementation CI](checks/implementation-ci.json) also passed.
The final documentation-only receipt SHA/CI is verified in the completion handoff.

Forced uninstrumented hero preparation is 4.872 s compiler / 4.89 s wall,
659.1 MiB peak RSS; package 3,873,651 B, two Full pages/32 MiB, 5,347 charts,
167,960 receivers, 495 probe slots/260 valid. The diagnostic bake takes 5.952 s
including evidence I/O and reproduces the package; this is not a measured solver
regression. Fresh normal samples have **360/360 nonzero scene-draw frames** each.
High room median CPU frame/event loop is 7.315/7.676 ms, p95 values retained,
31 accounted scene draws and 433.3 MiB peak RSS. No GPU execution time, physical
display cadence, other hardware result or quality/performance improvement is
claimed. Initial zero-counter measurements remain preserved and excluded.

The expanded evidence places sofa triangular tones in stored illumination and
physical direct transport; light-colored resin is severely underlit. At its authored
X/Z anchor the hall panel's horizontal footprint distance is 5.2 m, beyond its 5 m
falloff range. This qualifies direct coverage at one point, not all model/indirect/sky
support. Finite-shape sampling and soft shadows already exist. The entity
field is genuinely prepared. Window opening/jamb queries remain coherent.
No full-white nonemissive model, new physical leak or settled quality-switch
failure is demonstrated. A ready-gated capture cannot exclude loading transients.
Stages 1–6 remain hero-only; Stage 7 is supported-map compatibility/reference
migration, not theme-by-theme concept polishing.

The exposed controls are workspace-write plus reviewed per-command escalation;
persistent Full Access and lead Extra High reasoning cannot be verified/set here.
The configured model is GPT-6.1-sol; the global reasoning setting is medium.
The three bounded specialists were explicitly GPT-6.1-sol/xhigh. The runtime and
compiler roles owned disjoint diagnostic code; visual evaluation stayed read-only.
The lead alone integrated/captured/tested/reported/staged/committed/pushed.
No callable `/goal` interface or installed goal skill was exposed; the supported
delegated workflow was used. These restrictions were reported before authoring.

## Revision and custody receipt

- Prerequisite: `22a78fc43822900cd866869f93c55d7796e2c812`, `Art-style`,
  with exact-SHA prerequisite main/branch CI verified successful.
- [Baseline implementation](https://github.com/csd113/Places/commit/d24d278c8b5a248c5ee66efd8aaca063f0e16d13)
  pushed to `origin/Art-style`; [baseline CI](https://github.com/csd113/Places/actions/runs/37723638291)
  passed in 21m14s.
- Integrated diagnostics, final reports/journal and exact remote/CI verification:
  [implementation commit](https://github.com/csd113/Places/commit/2050fb23f1c6be75b0ee1b248b1a2623b36e37f7)
  is pushed and exact remote SHA verified.
  [Implementation CI](https://github.com/csd113/Places/actions/runs/37728914254)
  **passed** (26m54s job); [machine receipt](checks/implementation-ci.json).
  Native evidence retains its actual precommit
  Git/source/binary identities; it is not relabeled as a later-commit capture.

The inherited `/usr/bin/caffeinate -di` PID **88945**, started 2026-10-07,
remains active with both idle assertions. No duplicate inhibitor was created.
Custody record: `/tmp/places-art-style-queue-custody.json`. Final worker/job/process
checks and parent custody transfer follow push verification. Build artifacts stay
for the unfinished serial queue; no `cargo clean`, branch/worktree/tag/release or
Stage 2 work.

The original audit/implementation work and evidence reviews are collected. Three
subsequent read-only review requests remained `pending_init` after interruption
and explicit stop messages; they have no write/Git/build/native authority. Their
final controller state is reported at completion rather than claimed joined.
At 05:23 UTC a separate interactive shell (PID 51870) opened `target/debug/places`
(PID 62138), outside the Stage 1 capture launchers. It was left untouched; the
final process check will distinguish this session from task-owned jobs.
