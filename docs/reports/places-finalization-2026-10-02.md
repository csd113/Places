# Places desktop finalization — 2026-10-02

Lantern Hollow is playable, compiled and visually verified in the native Metal
renderer. The previously blocked presentation checks now pass with zero skips.
All four bundled packages are current and decode. The movement owner's completed
changes are integrated; Rust 1.99.0 is pinned and strict lint passes. The complete
canonical desktop gate passes, including native presentation with
zero skips. A fresh-release Python follow-up closes a discovered binary-order
gap. The saved Rust 1.99 executable also passes native showcase captures, quality
recovery and all 16 final presented performance cases.

## Current checkout and launch

The sole checkout is `/Users/connordawkins/Documents/GitHub/Places`, on `working`.
The showcase was already merged into `main` at
`a53696eed70b45b397f83dc58265aeeb740158a5`. Subsequent map, movement and compiler
work stays on `working` under the later branch restriction. No main update,
release, auto-merge or new branch is part of this finalization.

```sh
/Users/connordawkins/Documents/Codex/Recovery/Places-finalization-2026-10-02/launch-lantern-hollow.sh
```

The saved executable loads assets/packages from the sole checkout and remains
runnable after build cleanup. To rebuild from source, run `cargo build --release`
there, then `PLACES_ASSET_ROOT="$PWD" PLACES_LEVEL=lantern_hollow ./target/release/places`.

The Level Select menu also exposes **Lantern Hollow** and the new **Movement Test**.
WASD moves, mouse looks, E operates doors, Space jumps, and C crouches. The
default-off **Use Low-quality lighting** graphics option retains selected texture
quality and recovers baked lighting when disabled.

## Final authored content

The inspected attached reference informs four furnished cottages facing a modular
road, connected sidewalks and porches, fenced yards, pumpkins, a left-side pond,
dense forest and five converging trails. Seven seated skeletons surround the
animated, illuminating campfire. Rock faces and closed gates physically contain
the playable area. The final layout contains 54 disjoint rooms, 1,076 props,
247 trees and 459 grass placements, with no cars.

The 14 modular models are road_straight, road_unmarked, road_dash, sidewalk,
sidewalk_corner, curb, curb_ramp, sidewalk_transition, rock_face,
rock_face_variant, boulder, campfire, road_gate and stump_seat, under catalog IDs
`outdoor:showcase_*`. Existing home furnishings, vegetation, streetlights and
skeletons are reused. Both original sheet ghosts have attached facial geometry
that shares their cloth deformation; three sheet cats are placed in the map.
Asset checks cover animated clearance, exported clip seams, normals, UVs and GLB
accessors. Native front and oblique views show visible eyes and mouths.

The prior receiver-coordinate, model-chart and package-bound fixes remain in
place. Lighting comes from the compiler's visibility-tested HDR surface transport
and the established animated probe path. No brightness floor or compensating
emissive treatment is introduced. Later road/porch repairs and the movement
handoff's physical water rim are retained.

## Movement handoff

The movement owner completed its task before integration. Nineteen incoming
files, including all four compiled packages, were copied outside the repository
and SHA256-verified before commit `07e03d9`.

The changes repair falling beyond authored floor edges, candidate-support
headroom, crossed-top landing, stair pitchline descent, airborne body collision,
and the narrow physical water rim. They add the authored/compiled Movement Test
map, native capture harness and regression coverage. Walk, jump, gravity and
swim tuning constants are unchanged. Details and station inventory are in
[`MOVEMENT_DIAGNOSTICS.md`](../MOVEMENT_DIAGNOSTICS.md).

## Native showcase acceptance

- Final Rust 1.99 Metal bootstrap/presentation: **26 passed, zero skipped**,
  191.739 seconds. The earlier pre-upgrade run also passed all 26.
- Thirty-six native capture runs cover all four interiors, street, fire, pond,
  gates, both ghost models, overview, nine recorded walking routes and live quality
  recovery. High/Medium/Low and High with Low lighting were visually inspected. Eight
  additional captures from the saved Rust 1.99 binary refresh the final hero
  views and independently repeat eight presented quality installations.
- Nine position-audited native routes pass: enter each cottage from the road,
  strafe through its furnished interior, reach the bedroom and return; crouch/swim
  across the pond and exit; traverse the winding trail to the fire; repeatedly
  jump against both gates and the northern forest rock boundary.
- Eight quality installations retain level identity, player position, 20
  characters, four interactables and 11 routes. Each ready request subsequently
  records real presentation. The initial-level `scene_presented` marker is separate
  from the normal `present` events used by same-world graphics changes.

These are actual rendered/native controller runs, not manual keyboard-feel
acceptance. Manual keyboard/mouse playtesting and allocation profiling have not
been performed.

## Presented performance

Sixteen final serial runs use the preserved Rust 1.99 executable and measure 600
frames after 120 warmup frames, twice per camera and setting. Host: Apple M2 Pro,
Metal, 16 GB RAM, 1280×720 logical window, 2560×1440 drawable. Vsync is disabled;
all 16 logs confirm actual Immediate presentation after initial Fifo setup. Every
run installs the expected level and quality with nonzero draw calls.

The table gives the mean of two per-run frame medians and p95 values, in ms.

| Selected quality / lighting | Street median / p95 | Campfire median / p95 | Street / fire draws |
|---|---:|---:|---:|
| Low / Low | 2.567 / 4.120 | 2.042 / 2.669 | 783 / 80 |
| Medium / Medium | 2.630 / 3.945 | 2.005 / 2.420 | 761 / 80 |
| High / High | 2.558 / 3.766 | 2.015 / 2.292 | 761 / 80 |
| High / Low override | 2.623 / 4.298 | 2.131 / 2.622 | 783 / 80 |

Four fresh-process runs per setting, with warm filesystem caches, report:

| Selected quality / lighting | Mean request-to-ready | Mean process-to-ready | Maximum peak RSS |
|---|---:|---:|---:|
| Low / Low | 676.7 ms | 1280.2 ms | 378.9 MiB |
| Medium / Medium | 1544.2 ms | 1867.6 ms | 1038.6 MiB |
| High / High | 1679.8 ms | 2000.9 ms | 1175.5 MiB |
| High / Low override | 645.0 ms | 967.8 ms | 453.4 MiB |

These are CPU frame/loop timings and CPU decode/renderer-install measurements,
not GPU completion latency or an isolated quality ranking. Normal desktop and
other-project activity is not controlled. Earlier pre-upgrade runs had an unrelated
benchmark occupying a CPU core; they are retained separately rather than used in
this final table. No compiler-upgrade performance gain is claimed.

Raw JSON, frame CSVs, logs, load traces and non-sensitive CPU process metadata are
preserved under recovery `qa/showcase-performance-rust199/` and its adjacent log.
The helper in recovery `helpers/night_showcase_bench.py` accepts the retained
binary and a fresh output directory without depending on deleted capture state.

## Rust 1.99.0

`rust-toolchain.toml` selects exact `1.99.0`, minimal profile plus Clippy and
rustfmt. Cargo's `rust-version` is `1.99`; README and verification prerequisites
match. No dependency or lockfile upgrade, machine-wide default change or lint
policy relaxation is required. No existing CI configuration is present.

Compiler-introduced strict lint diagnostics are fixed using fused arithmetic,
eligible const functions, typed empty comparisons, standard tuple/array
conversions and shared success branches. Regression expectations use the same
numeric formulas. Commits `06819a7` and `1fb1be2` contain these changes; the user
made the former GitHub Desktop commit while work was in progress, and its existing
title is retained. Its contents are compiler/lint changes rather than an additional
map geometry repair.

Strict formatting and full-policy Clippy pass. Canonical validation is invoked
with bounded concurrency and optimized test code, retaining debug assertions:

```sh
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2 CARGO_PROFILE_TEST_OPT_LEVEL=3 sh tools/verify.sh
```

This runs the complete workspace suite, required ignored atlas/GPU audits,
asset/generator checks, incremental compilation/currentness/decoding for all
four maps, Python/package/geometry checks, native compiled-build and presentation
tests, and the release build. It passed: 1,912 library tests plus three workspace tests, zero failures; the
library run took 1,666.92 seconds. The full suite initially ignores 21 diagnostics;
the required atlas audit and two GPU audits then pass explicitly. The remaining
18 optional diagnostics are outside the canonical gate.

The Python suites pass 175 tests in total, including 48 package, 16 packaging/GLB,
41 tooling, 14 geometry, 10 native compiled-build and 26 presentation checks.
Generator/current-asset checks and all four current/decode checks pass.

A gate-order issue was identified: geometry and packaging tests could select a
previously existing release application before the script rebuilt it. The release
build now runs immediately after the full Rust suite, before Python/native checks;
the documented command order matches. After the first script run passed, all
139 affected pre-release Python tests were rerun against the fresh Rust 1.99
application and passed in 387.247 seconds. The final GPU and 36 native tests already
used that new executable. Shell syntax and diff checks pass. This follow-up closes
the stale-binary gap without repeating the unchanged expensive Rust suite.

## Packages and preservation

| Package | Bytes | SHA256 retained through upgrade |
|---|---:|---|
| lantern_hollow | 115,529,161 | `cf94109ce18d84bb4bf2227dfdf29ff8bcc27ed26494f5931a769c242e9e2683` |
| model_zoo | 82,395,130 | `7601d77f7ac3a3bfc719603d17c97eba0516374cf6552877f3cb264d69b853cb` |
| movement_test | 43,641,558 | `9f43945c0fa2e87eecff130e087bcb0b0710830f59ee6b64389e8383be0fabb5` |
| places_demo | 113,120,220 | `786b625945978807294b2ab7dd1534ba20b1951695d9a1c6ba6a53cb7b5ae4aa` |

The final packages pass `verify --require-current` and complete decode. All
four canonical incremental builds reuse their packages and SHA256 checks retain
these exact bytes; no lighting solver revision
or discretionary rebake is introduced for lint fixes. The Lantern/Demo packages
use Git LFS. A fresh clone needs `git lfs install` and `git lfs pull`.

Persistent recovery root:
`/Users/connordawkins/Documents/Codex/Recovery/Places-finalization-2026-10-02`.
It already contains verified incoming changes and pre-upgrade binaries. Final
1.99 binaries, screenshots and unique diagnostics are preserved there with the
SHA256-verified `final-manifest.json` before authorized cleanup: 3,700 files and
5,081,293,711 bytes verified. Cleanup runs only
after all validation and preservation, with no process using this checkout's build
output; it does not remove the saved executable or repository assets/packages.

## Screenshot delivery and limits

Fresh native PNGs are staged at
`/Users/connordawkins/Documents/Codex/2026-10-01/task/showcase-final-delivery-20261002`:
overview, street, campfire, pond, furnished interior, sheet cat and human ghost,
plus the earlier matching interior before image. The seven final images are refreshed native 3024×1676 captures from the saved
Rust 1.99 executable; the before image is explicitly historical.

The current unmodified Library upload helper failed during authenticated
`tools/list` discovery with `TLS`. No prepare, transfer, write or finalization
occurred; no fresh Library IDs exist. The request retains the five original
replacement identities/version guards. Existing Library images are not described
as these final captures, and no TLS bypass was attempted.

Fire has five animated flames and supported point illumination; smoke, embers
and audio are not added. Animated characters retain probe lighting without dynamic
shadow redesign. Dense foliage receives baked light with coarse chart LOD and
does not cast transport shadows; tree trunks remain collidable. Furniture, cliffs,
gates, boulders and stumps retain authored occlusion and shadows. The desktop
target and low-poly art style remain unchanged.
