# Prop interaction repair: Mac validation

Recorded 2026-11-05 on Apple M2 Pro/macOS 27.2, against `087d822`.

Held motion now samples control history at each 120 Hz physics tick. Swept
previews retain the solved pose, route and obstruction timer across fractional
frames. A supported-contact guard prevents a held prop from forcing another
prop into scenery. Free pushes still require actual solved contact.

Nearby unequal dynamic masses receive additional contact convergence. The
medium-ratio profile uses damped contacts and 24 normal/four relaxation passes;
high ratios retain 64/64. Kinematic held mass and fully sleeping pairs do not
enable this profile. Authored mass, inertia, friction, restitution, admission
and save semantics are retained.

## Correctness

- Formatting, strict all-target/all-feature Clippy, locked release workspace
  build, and 481 workspace tests passed. Six focused regressions use small CPU
  fixtures; CI performs no map bake or native launch.
- Forty-eight push trajectories cover 30, 60, 120, 144, 240 and 480 Hz control,
  enabled/disabled player contact, and four mass combinations. All establish
  positive pair impulses; maximum target-position spread is 6.44 micrometres.
- The representative 25 g/5 kg and 1 kg/1000 kg pairs settle without overlap.
  Clear-floor mixed stacks of 32/64/128 bodies also sleep without overlap.
  The 128-body malformed-spawn control settles with 0.83 mm maximum overlap,
  compared with 92 mm baseline after five simulated seconds.
- Sixteen direct-binary native cases cover real contacts, supported jams,
  stacking, pickup, throws, blocked release, dormancy, recovery, clocks,
  transitions, surfaces, checkpoint carry, automatic drop and elevator cargo.
  The corrected crouched free-push fixture records positive sphere/cup impulses
  and 27 mm cup travel; the standing table jam retains geometric clearance.

## Cost and limits

These are isolated `ItemWorld` CPU milliseconds, release/all-features,
including optional parallel physics. Dense stacks alternate 25 g/5 kg, with
20 cm cubes in four-by-four layers, friction 0.6 and zero restitution/damping.
Each mean/p95 averages three runs of 180 ticks after 30 warmup ticks.

| 128-body workload | Workers | Baseline mean / p95 | Repair mean / p95 |
| --- | ---: | ---: | ---: |
| Spawn intersects table | 1 | 0.560 / 0.716 | 1.507 / 1.801 |
| Spawn intersects table | 4 | 0.393 / 0.528 | 1.482 / 1.762 |
| Clear-floor mixed stack | 1 | 0.764 / 1.071 | 0.572 / 1.921 |
| Clear-floor mixed stack | 4 | 0.507 / 0.717 | 0.614 / 2.026 |

The superseded stiff solver candidate cost 4.269 ms in the first workload;
this repair reduces that cost by 65%. Remaining stress cost and settling tails
are real tradeoffs. The baseline leaves deep overlap and many bodies awake.
The 128-body held-pile case costs 1.275 ms per 60 Hz control call containing two
physics ticks, versus 1.060 ms baseline and 5.668 ms for the superseded candidate.
After its settling tail, held-body overlap is zero; another pile pair retains
6.07 mm overlap.

Home carry, Office CRT/paper contact and Pool cart/bottle contact use current
Quiet Collection sources, ordinary controls, two reversed-order comparisons,
seven ready seconds and real delta. Native CPU measurements use matched Fifo
presentation at 1280×720 with static lighting. They do not establish uncapped
FPS or results on another platform. Default-feature native performance was not
separately measured.

Mean physics cost per native control frame was Home 0.108→0.176 ms,
Office 0.077→0.132 ms and Pool 0.131→0.127 ms. Office and Pool establish
positive prop-pair impulses; Home establishes ordinary pickup, carry and drop.

This is a bounded repair: admitted 1 g/1000 kg pairs can still sleep with about
100 mm overlap, and millimetre-scale props retain floor-contact limitations.
Transient drop overlap is reported separately from resting overlap.

Reproduce the build with `cargo build --locked --release --workspace --all-features`;
run native tools explicitly against the resulting binary. The measured runtime
source fingerprint is `a7f366a4bb5feb097d6c9e35bb4b5fac9edeb21f11a16aa130d6b04e77bef483`;
the player binary SHA-256 is
`2ab6755c118c3a53523cdaacc2d33e3cf4d0aba4fd6f0a7e6b7cfea5f6f41faa`.
