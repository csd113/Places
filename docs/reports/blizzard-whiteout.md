# Prompt 7 — Blizzard / Whiteout Weather

Winter retains its normal light snowfall. A separate, launchable Winter review
map selects a severe blizzard with directional snow, strong wind and short-range
whiteout. This commit implements Prompt 7 only; prompts 8 and 9 remain queued.
Starting HEAD was `d095d1edc7e33e3aa915cb2d2c05b4b5fad86f52`; its documentation
[CI](https://github.com/csd113/Places/actions/runs/37610105298) passed.

## Configuration and implementation

The existing reusable `weather.kind = "snow"` architecture now also exposes
`intensity` (0–1 seed fraction), `storm_severity` (0–1), `visibility_m` (2–100 m)
and `fog_color` (three 0–1 display-space components). Existing wind X/Z velocities
remain limited to ±0.5 m/s in calm weather and allow ±20 m/s in a storm. The
canonical authoring guide documents every range and shelter limit.

The review source uses severity 1, visibility 5 m, wind `[8,3]` m/s, radius/height
6 m, size 2.5–6 cm and downward speed 0.8–1.8 m/s. Its seed budget is the same
1,400 as calm Winter: High/Medium/Low evaluate 1,400/1,050/700 stable seeds.
Intensity can reduce that prefix independently of fog. No enormous particle
count or map-wide simulation is used. Velocity projected into the camera-facing
plane stretches nearby quads into wind-aligned streaks; the existing committed
square RGBA flake PNG, full UVs and alpha border remain unchanged.

Long-distance occlusion comes from exponential-squared extinction of the outdoor
part of each sightline: `T = exp(-(2 * severity * outdoor_distance / visibility)^2)`.
At full severity, contrast transmission is approximately 85% at 1 m, 24% at 3 m,
1.8% at 5 m and negligible at 10 m. World geometry, props/characters, translucent
ice and decals share the optical model. Emission is attenuated before bloom;
nearby warm lights glow without distant lamps leaking through the storm. Sky
blending uses the square root of severity and hides the aurora at full severity.
Calm and absent weather use an exact zero-density bypass.

Flat/gable room volumes and occluding roof slabs suppress flakes and remove
sheltered distance from optical depth. Convex ray intersections are unioned so
overlapping rooms/eaves cannot double-subtract distance. Fully indoor rays stay
clear, while exterior views through openings fog only after leaving shelter.
Both validators reject storm maps exceeding 32 combined ceilings/roof slabs.
The GPU uses direct uniform reads, bounds rejection, an indoor early return and
an array-free interval sweep; the CPU mirror uses fixed scratch and a sorted
union. Transient weather is cleared for probe/planar capture resources and
applied once on the presented surface. Existing lighting/bakes remain intact.

There is no screen-space noise, generated texture, camera shake or new asset
catalog entry. Existing snowfall fades, frustum rejection and shelter suppression
are reused; optically extinguished flakes are culled before vertex upload and
reported as culled rather than roof-sheltered. Arbitrary prop/tree/dynamic roofs
are not traced; author a ceiling/slab for weather shelter. Snow has no collision
or accumulation. This is deliberately a bounded architectural weather model.

## Native review and performance

Actual pixels of the existing Winter concept sheet were inspected before the
implementation. The real SDL3/wgpu player ran on Apple M2 Pro / Metal, with
960×540 logical / 1920×1080 drawable windows, isolated accepted settings and
load traces confirming the installed map and quality. Final captures cover all
three qualities (39 severe and 24 calm images), outdoors/indoors, doorway traversal, camera turns and held
strafing. Additional storm views cover nearby lamps, buildings at two distances,
close forest and the frozen pond. Quality cycling checks High → Low → Medium →
High without resetting the snowfall clock. The preserved launcher is also tested.

Inspected native frames show wind-aligned white streaks against a whitened sky,
nearby snow terrain and trees at short range, and rapid loss of distant forest
and buildings. The lodge changes from a readable silhouette to nearly invisible
with a small increase in distance. Nearby warm string lamps remain legible;
the pond loses distant surface detail. Indoor walls/furniture stay clear and
outside snow remains visible through the doorway. Low retains its existing
darker indoor lighting; storm visibility is independent of quality. Calm Winter
retains sparse flakes, clear distances and aurora. No scene/art integration was
changed to obtain those results.

Final severe campaign totals, with diagnostic projection enabled:

| Quality | Evaluated seeds | Peak submitted | Maximum projected quad coverage | Worst view p95 CPU sync |
| --- | ---: | ---: | ---: | ---: |
| High | 1,400 | 172 | 29.896% | 163 µs |
| Medium | 1,050 | 132 | 26.185% | 223 µs |
| Low | 700 | 85 | 19.246% | 193 µs |

Calm peaks are 213/157/107 submitted, with maximum coverage 0.795/0.737/0.422%
and worst view p95 CPU sync 120/90/51 µs for High/Medium/Low. Sync includes
shared billboard generation, diagnostic projections and queue submission;
diagnostics are disabled in normal play and A/B measurements.
All recorded native runs have zero retained vertex/group capacity growth.
Winter retains 39,200 bytes of seed storage and 218,400 bytes of snow GPU
vertex/index reservation. A storm uniform is 1,568 bytes, appended to world and
decal environment records; the sky record adds 16 bytes. CPU interval scratch is
256 stack bytes. Seed, vertex/group and optical scratch storage do not allocate
per frame; queue staging and unrelated engine allocations are outside this claim.
Only surviving particle vertices upload. Coverage sums projected quad rectangles
before texture alpha/depth rejection, so it is a conservative fill estimate,
not a hardware fragment counter.

The final native same-binary square A/B uses VSync off, GPU completion, 120 warmup
frames and 600 measured frames per run in on/off/off/on order, with diagnostics
disabled. Each side contributes 1,200 frames per quality. Storm results:

| Quality | Weather off median | Storm on median | Median difference | Mean difference |
| --- | ---: | ---: | ---: | ---: |
| High | 7.044 ms | 8.494 ms | +1.451 ms | +1.153 ms |
| Medium | 6.005 ms | 7.832 ms | +1.827 ms | +0.989 ms |
| Low | 4.754 ms | 7.171 ms | +2.417 ms | +1.159 ms |

The equivalent final calm campaign measured mean increments of 0.016/0.274/0.371 ms
and medians of 6.816/5.674/5.281 ms for High/Medium/Low. Host timing was noisy:
running the preserved original Prompt 6 binary on the same host produced calm
on medians of 7.320/5.521/4.826 ms and off medians of 6.809/6.102/4.546 ms. These
controls support substantial host jitter and no clear large calm regression;
negative differences are not evidence of a speedup. Completed-frame timing
includes CPU, driver and GPU work rather than isolated GPU timestamps. These
observations cover one Mac, not every supported desktop. Early native shader
runs with costly copied interval storage are retained as superseded evidence;
final captures and A/B manifests identify the final `ebf879d3…` binary.

## Validation and preserved deliverables

The final `env -u PLACES_ASSET_ROOT RUSTC_WRAPPER= CARGO_INCREMENTAL=0 sh tools/verify.sh`
exited 0. Formatter, locked workspace/all-target/all-feature check, strict
development/release Clippy, workspace/all-feature Rust tests (2,021 library,
six binary/integration, zero failures, 23 existing ignored; 314.42 s library),
release build, asset/generator validators, all bundled currentness/decode gates,
focused showcase/static-lighting tests, the explicit atlas-plan test, both
explicit native Low-lighting tests and whitespace checks passed. The 198 Python
cases include 10 compiled-build cases (43.363 s) and 26 native bootstrap cases
(146.054 s): both native suites report zero failures and zero skips. The Mac was
unlocked; no lock guard was bypassed. Expected failing-tool fixtures emitted
their diagnostic errors while their assertions passed.

The prior default `cargo test --workspace` also passed 2,021 library cases,
six binary/integration cases and doctests, with zero failures and 23 existing
ignored cases. New focused coverage checks extinction distances, calm identity,
gable/indoor/doorway rays, overlapping/parallel shelter intervals, WGSL/uniform
layouts, quality/intensity/streak/storage bounds, authoring limits and diagnostic
counter semantics. No lint levels or skipped-test guards were weakened. A
multi-start quality-cycle campaign was rejected by the existing harness because
a Medium start was compared to the cycle's final High state; that attempt is
retained but excluded. The supported High-start doorway cycle passed, explicitly
verifying budgets 1,400 → 700 → 1,050 → 1,400, monotonic seconds and zero growth.

All five shipped package hashes remain unchanged, as do Winter source, assets
and catalog. Normal compilation reported Winter current in 1.093 s. The review
map compiled once in 226.086 s under concurrent CPU validation, without forced
bakes or altered compiler semantics. Its off/medium/full mesh, props, lighting,
collision, navigation, lightmap texels, irradiance and probe payloads match Winter;
only weather/identity semantics and identity-derived lightmap metadata differ.
Both Winter and review currentness/decode checks pass.

`debug-maps/blizzard-20261007/README.md` provides build/capture commands. Its
ignored `evidence/` directory preserves sources, both calm/severe packages, all
required assets, final player/compiler/SDL binaries, launcher, native images,
CSV/load logs, final A/B manifests and raw validation results outside `target`.
The preservation manifest verifies 400 matching files (666,551,912 bytes).
Launch either mode with:

```sh
python3 debug-maps/blizzard-20261007/evidence/launch.py winter
python3 debug-maps/blizzard-20261007/evidence/launch.py blizzard_review --quality high
```

Existing snowfall, movement and compiler debug archives, external Office and
consolidation evidence, and shared build artifacts remain intact for queued
tasks. Exact publication SHA, remote verification and CI result are recorded in
the final handoff and local `evidence/handoff.json` after the separate commit.
