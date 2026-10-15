# Bowling lane polish — October 15, 2026

![Native Bowling Alley view](../images/bowling-lane-polish/lanes.png)

The Bowling Alley in Cosmic Lanes & Arcade uses the existing half-resolution
planar reflection path for a restrained polished maple finish. Only
`arcade:lane_wood_01` changes; its four lanes and wooden approach share the finish.

| Material control | Before | After |
| --- | --- | --- |
| Specular | 0.26 | 0.36 |
| Shine | 0.52 | 0.60 |
| Roughness (`1 - shine`) | 0.48 | 0.40 |
| Reflection strength | 0.12 | 0.28 |
| Reflection mode | Probe | Planar |

The existing rough planar filter softens reflected nearby lights.
The wood PNG, UV scale, lane arrows, foul lines, geometry, light placement and
physical friction remain unchanged. Carpet, gutters, walls and other materials
retain their original definitions. This introduces no shader changes, new
textures or dependencies. The renderer draws at most one half-resolution
reflection view per frame, choosing between the lane plane and existing
restroom mirrors; the former lane probe is no longer required. Reflections
remain controlled by the existing Full/Medium/Off setting and Static lighting
continues to omit the surface response.

Reflection priority now measures distance to each surface's finite bounds.
Previously, the floor's infinite plane could win from inside the restroom and
disable its nearby mirrors. The bounds calculation retains the same frustum
gate, deterministic ordering and one-view limit without allocating per frame.

## Reproduction

Build the player and compile the actual production map using the normal pipeline:

```sh
RUSTC_WRAPPER= cargo build --locked --release --bins
target/release/places-compile build levels/retro_arcade_demo.json \
  --out assets/levels/retro_arcade_demo.placesmap --variants off,full --workers 8
target/release/places-compile verify levels/retro_arcade_demo.json \
  --package assets/levels/retro_arcade_demo.placesmap --require-current
```

Use `tools/bench/frame_time.py` with `--binary target/release/places`,
`--capture-frame 60 --warmup 15 --size 960x540` and a new `--out` directory.
The macOS drawable is 1920×1080. Camera arguments are degrees; spawn arguments
are X, eye Y, Z, yaw.

| View | `--spawn=` | `--camera=` |
| --- | --- | --- |
| Aligned with the ceiling fixtures | `10.65,1.6,22.2,0` | `0,-24` |
| Front | `12.65,1.6,22.2,0` | `0,-12` |
| Oblique, including carpet boundary | `18.3,1.6,21.7,330` | `330,-18` |
| Downward, grain and arrows | `12.65,1.6,19.2,0` | `0,-55` |
| Grazing, from the concourse | `14.65,1.6,24.6,0` | `0,-3` |
| Existing restroom mirror | `-2.7,1.6,5.6,180` | `180,-4` |
| Close fixture reflection and grain | `10.65,1.6,20,0` | `0,-70` |

Matched baseline runs use a preserved original catalog and original package via
`--asset-root`. Native logs, package/binary hashes, frame CSVs and capture
receipts are kept locally under `tools/bench/results/lane-polish/`.

## Native results

The production map was inspected from the front, oblique, downward, grazing,
fixture-aligned and close views. Reflected light stays dim and softened while
the maple grain, arrows and foul line remain readable. A centered camera can
place fixture reflections in the gutters; the aligned and close views show the
reflection on the wood itself.

| Original finish | Polished finish |
| --- | --- |
| ![Original wood](../images/bowling-lane-polish/finish-before.png) | ![Soft fixture reflection over the grain](../images/bowling-lane-polish/finish-after.png) |

Matched frame sequences also checked Medium, Off, low texture filtering with
Full lighting, and Static lighting. The complete Static image and the complete
restroom-mirror image are pixel-identical to the original. The oblique carpet
boundary crop (1530,650)–(1870,1030), in the 1920×1080 drawable, is also identical.
Only the lane material differs in the catalog. The prepared geometry, props,
lightmaps, irradiance, collision and navigation records have identical content
hashes; the former lane-probe record changes because it is no longer needed.
Package verification reports current provenance, no differences and no warnings.

Both final sweep runs presented all 540 ready frames with zero skipped frames;
the close finish run presented all 60. Occluded exploratory runs were rejected
and excluded. Temporary macOS bundles contain exact copies of their respective
release executables so the windows can be foregrounded before loading completes.
The local runner adds a preparation delay solely for window activation; timing
starts after loading and warmup.

## Performance

Apple M2 Pro / Metal, 1920×1080 drawable, High textures, Full lighting and
reflections, bloom enabled, Immediate presentation. Original and final players
ran serially without compilation or another game process competing. Each run
presented 3,600 ready frames with zero skips. Six 600-frame camera blocks
alternate front and oblique views; discarding the first 120 frames of every
block leaves 1,440 steady frames per view per player. GPU sample IDs are
deduplicated. These are elapsed GPU pass spans, including scheduling stalls,
not active hardware utilization.

| View | GPU median before → after | GPU p95 before → after | CPU loop median before → after |
| --- | --- | --- | --- |
| Front | 2.620 → 2.496 ms | 3.330 → 2.921 ms | 3.263 → 2.127 ms |
| Oblique | 2.711 → 2.718 ms | 3.312 → 3.127 ms | 3.326 → 2.194 ms |

The final GPU p95 stays below 3.2 ms in both views on this machine. Base scene
draws stay 161/173 for the two views; both players use exactly one reflection
pass. The existing one-view budget is preserved.
Raw CSVs, renderer identities and binary/package hashes are retained in
`timing-verified-before/`, `timing-verified-after/` and `performance-summary.json`
under the local results directory. Camera blocks are in `timing-cameras.json`.
The native sweeps use `sweep-before.json` and `sweep-final.json`, with graphics
changes at frames 301 (Medium reflections), 361 (Off), 421 (Full), 422 (Low
filtering) and 481 (Static).

## Validation

The following source checks passed:

```sh
cargo fmt --all --check
RUSTC_WRAPPER= cargo clippy --workspace --all-targets --all-features -- \
  -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo
RUSTC_WRAPPER= cargo test --workspace --all-features
RUSTC_WRAPPER= cargo test --workspace
python3 tools/assets/validate.py
git diff --check
target/release/places-compile verify levels/retro_arcade_demo.json \
  --package assets/levels/retro_arcade_demo.placesmap --require-current
```

All-features: 2,339 library tests passed; default features: 2,332 passed.
Both also passed nine binary/integration checks; each retains 19 existing ignored
library tests. Asset validation reported zero warnings. One regression test uses
two bounds and two cameras to protect mirror/floor selection. It runs without
assets, GPU work or baking; native comparisons exercise the resulting appearance
and existing renderer gates directly.

The shared catalog's hash is part of every compiled package's integrity
contract. After changing this material, other previously compiled local maps
also need recompilation against the new catalog before they can be loaded.
Packages are generated local artifacts and are not tracked in Git.
