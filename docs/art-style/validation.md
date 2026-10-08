# Stage 1 validation — 2026-10-08 UTC

No Rust renderer/compiler behavior, existing asset, concept PNG, shipped map,
platform backend, quality limit or validation rule changed. Changes are the
ordinary hero fixture, camera manifest, small native capture wrapper/contracts,
audit/ledger, raw evidence and journal entry.

| Command actually run | Result / evidence |
| --- | --- |
| `cargo build --release` | Initial inherited sccache execution failed (`Operation not permitted`), including an escalated attempt. `RUSTC_WRAPPER= cargo build --release` passed in 1m37s; [log](checks/release-build.log). |
| `RUSTC_WRAPPER= cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0; [log](checks/clippy.log). |
| `RUSTC_WRAPPER= cargo test --workspace` | Exit 101 after 2,022 library tests passed / 23 ignored and successful binary/CLI targets. All three `tests/list_levels.rs` tests fail on the explicitly inherited stale local packages; [full log](checks/cargo-test-workspace.log). This command is not reported as passed. |
| `RUSTC_WRAPPER= cargo test --workspace --test macos_cpu_port` | Exit 0, 1 passed; [log](checks/cpu-port.log). |
| `RUSTC_WRAPPER= cargo test --workspace --doc` | Exit 0, 0 doc tests; [log](checks/doc-tests.log). |
| `cargo fmt --all --check` | Exit 0; final independent receipt below. |
| `python3 -m unittest tests.test_art_style_hero` | Exit 0, 5 contract tests, including quality mismatch rejection. |
| `python3 tools/assets/validate.py --quiet` | Exit 0, including the hero and existing source maps. No catalog-reference errors. |
| `python3 tools/textures/build.py --check` | Exit 0, existing preferred-size warnings remain informational. |
| `python3 tools/props/build.py --check` | Exit 0; no source/model asset changed. |
| `python3 -m py_compile tools/bench/capture_art_style_hero.py tests/test_art_style_hero.py` | Exit 0; ignored reproducible `__pycache__` only. |

The last five formatting/Python checks have separate exit codes/output in
[final scoped receipts](checks/final-scoped-checks.json). Local data-bearing
package failures are recorded in [fresh currency checks](deferred-package-checks.json)
and [the exact ledger](deferred-ledger.json). User instructions explicitly defer
nonblocking historical map/reference migration to Stage 7. No file was moved out
of discovery, stale package deleted/replaced, test altered/skipped with a new
filter, assertion weakened, or existing warning suppressed. Scoped targets supplement
the recorded failed whole-workspace run; they do not relabel it green.

## Hero preparation and native checks

```sh
target/release/places-compile build tests/fixtures/levels/art_style_hero.json \
  --out debug-maps/art-style-hero/evidence/art_style_hero.placesmap --workers 12 --force --json
target/release/places-compile build tests/fixtures/levels/art_style_hero.json \
  --out debug-maps/art-style-hero/evidence/art_style_hero.placesmap --workers 12 --json
target/release/places-compile validate debug-maps/art-style-hero/evidence/art_style_hero.placesmap --json
target/release/places-compile inspect debug-maps/art-style-hero/evidence/art_style_hero.placesmap --json
target/release/places-compile verify tests/fixtures/levels/art_style_hero.json \
  --package debug-maps/art-style-hero/evidence/art_style_hero.placesmap --require-current --json
target/release/places --check-geometry --level tests/fixtures/levels/art_style_hero.json \
  --json docs/art-style/geometry.json
```

Every command exits 0. All three variants have no lightmap failure;
[integrity](package-validation.json), [currency](package-currency.json),
[inspection](package-inspect.json), [forced/reused build](compile-cold.json),
[reuse receipt](compile-reuse.json), [geometry](geometry.json) and its
[human log](geometry.log) are retained. Geometry reports zero errors and one
open-garden warning. This decorative region lies outside the sealed window;
it is not an enclosed playable destination. The warning stays visible and has
an explicit ledger entry. The normal `geometry_intent` annotation describes this
genuinely open garden and accounts for five missing-wall findings in the checker
receipt; it does not cover the remaining room-leak warning. The interior doorway
floors meet at the wall centre.

Native captures actually run:

```sh
python3 tools/bench/capture_art_style_hero.py --out docs/art-style/baseline/high
python3 tools/bench/capture_art_style_hero.py --out docs/art-style/repeat/high --views room,window,entities
python3 tools/bench/capture_art_style_hero.py --out docs/art-style/baseline/low --quality low --views window,entities
python3 tools/bench/compare_captures.py docs/art-style/baseline docs/art-style/repeat
python3 tools/bench/probe_lighting_report.py \
  debug-maps/art-style-hero/evidence/art_style_hero.placesmap --locations docs/art-style/probe-locations.json
```

Six High views, three repeats and two Low controls succeeded and were visually
inspected. Three High PNG pairs are byte-identical; [comparison](repeatability.json).
Native logs show M2 Pro/Metal, correct level, valid settings and no missing
textures/failed static assets. The dynamic chair uses the supported runtime
rigid-mesh path; its log correctly explains that an unskinned chair cannot be
posed. All output comes from ordinary engine rendering and stored PNG assets.
The forced final bake retains the exact package hash used by those captures.

Bounded resource/performance samples run High room/hall/entities and Medium/Low
room with 120 warmup + 360 recorded frames. Their limitations, the omitted initial
shared-load/mislabeled attempts and all valid resource costs are in
[performance](performance.md); zero-counter background loops are not scene FPS.
GPU timestamp timings, foreground presentation and other hardware were not
measured. No broad all-map bake or full desktop campaign was duplicated here.

## Prerequisites and final review

Clean entry was `Art-style` tracking `origin/Art-style` at
`22a78fc43822900cd866869f93c55d7796e2c812`. The parent integration receipt remains
`/tmp/places-branch-integration-20261008/integration-evidence.json`. Fresh
[main CI](https://github.com/csd113/Places/actions/runs/37720827363) and
[Art-style CI](https://github.com/csd113/Places/actions/runs/37720963325) both
completed successfully on that exact prerequisite SHA. Stage 1's own pushed
SHA/CI receipts are in [handoff](handoff.md).

Review includes the full task-owned diff, whitespace checks, native image review,
reference identity preservation, source/package correspondence, prior-asset diff
checks and absence of stage-specific Rust branches. Only the lead stages files,
commits and pushes. The three read-only reviewers finished without jobs or edits.
Final native/build/test process and inhibitor checks occur before custody release.
