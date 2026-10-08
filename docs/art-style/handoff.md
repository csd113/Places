# Stage 1 handoff — 2026-10-08 UTC

Stage 1 establishes a native visual benchmark and dependency audit. It does not
implement or claim a renderer/lighting improvement. Stage 2 has not started.

## Delivered

- [Hero source](../../tests/fixtures/levels/art_style_hero.json): compact furnished
  room, connected hallway, glazed garden, broad fluorescent/warm practical,
  existing material families, static props and an ordinary spawned chair.
- [Fixed camera/settings manifest](hero-manifest.json) and
  [native capture launcher](../../tools/bench/capture_art_style_hero.py).
  Launch interactively with `python3 tools/bench/capture_art_style_hero.py --play`
  after the explicit normal compile documented in [the report](README.md).
- Six raw High views, two genuine Low controls and three matched native repeats.
  The repeated room/window/entities views are byte-identical to the baseline;
  this demonstrates repeatability, not a visual gain.
- [Audit and working strengths](README.md), [Stages 2–7 dependency plan](gap-plan.md),
  [costs](performance.md), [exact deferred ledger](deferred-ledger.json),
  [validation commands/results](validation.md), and the dated entry in the
  [existing chronological journal](../style-upgrade-20261007/README.md).

## Acceptance and limitations

Normal release build, strict Clippy, all-variant hero compile, integrity/currency,
native capture, reference hash checks, asset validation, five capture contract
tests and scoped Rust checks passed. Geometry has zero errors and one retained
open-garden warning. No Rust renderer/compiler behavior or existing asset changed.

`cargo test --workspace` exits 101: 2,022 library tests pass, 23 are ignored;
three level-discovery tests fail on inherited stale local packages. Their exact
dependencies, current checks and Stage 7 resolution are in the ledger. They were
not deleted, hidden, repaired speculatively or relabeled as passing.

Forced all-variant compile is 4.872 s / 659.1 MiB peak RSS; package is 3,873,651 B.
High native room process peaks at 447.3 MiB; two Full atlas pages occupy 32 MiB.
There is no pre-hero cost comparison or claimed performance delta. Background
surface frames have zero scene counters, so a reliable foreground FPS/GPU-time
baseline remains open. Genuine native scene readback images succeeded.

The task exposes workspace-write with reviewed escalation; a persistent Full
Access setting and the lead's requested reasoning configuration are not
verifiable/settable here. Read-only reviewers were explicitly GPT-6.1-sol/xhigh.
No callable `/goal` interface was available; the delegated workflow was used.
These restrictions were reported before authoring and remain prerequisites for
the coordinator to resolve/verify at the next launch.

## Revision and custody receipt

Implementation and final journal commits will be linked here after their push
and applicable CI outcomes are verified. This initial receipt is still active
Stage 1 work, not a release of ownership.

The inherited `/usr/bin/caffeinate -di` PID **88945**, started 2026-10-07,
remains healthy with both idle assertions. No duplicate inhibitor was created.
Custody record: `/tmp/places-art-style-queue-custody.json`. Build artifacts and
the reproducible hero package remain outside tracked deliverables and are kept
for the unfinished queue. Final job/process checks and parent custody transfer
will occur after push verification. No `cargo clean` or Stage 2 work.
