# Places 0.10.0 prerelease

October 16, 2026. This source prerelease brings the completed 0.10.0 staging
work to main. Maps remain optional external content; a fresh installation opens
the menu without baking, downloading or inventing a world.

## Included work

- [Item physics](item-physics-0.10.0.md), [water behavior](water-0.10.0.md),
  an [outline interaction hand](interaction-hand-0.10.0.md) and the opt-in
  [developer console](developer-console-0.10.0.md).
- An occluded [flashlight and Office fixture response](flashlight-0.10.0.md),
  animated source-lit Pool caustics, [linked railings](modular-railings.md) and
  [soft bowling-lane reflections](bowling-lane-polish.md).
- Completed [Arcade/Shared/entity models](model-integration-0.10.0.md) and
  [Pool/Office/Winter/Outdoors deliveries](pool-office-winter-model-integration.md).
  The accepted inventory has 178 model records; original deliveries and editable
  sources are preserved separately from runtime assets.
- [Lighting work reduction](runtime-lighting-0.10.0.md),
  [Arcade performance fixes](performance-0.10.0.md) and
  [runtime preparation scheduling](multicore-0.10.0.md).
- A native `places-compile gui`: source/output pickers, explicit Static + Full
  lighting and reflection-probe baking, live per-pass counts, and cooperative
  cancellation. It uses existing SDL3 and the committed PNG font atlas. The GUI
  source left by the preceding task is preserved, with no new dependency.

Cargo is the version authority for the player menu/loading UI, diagnostics and
compiler. The macOS packaging script derives both bundle version fields from it.

## Final release validation

Passed on the final source/version inputs:

- `cargo fmt --all --check` and strict all-target/all-feature Clippy, including
  the additional all/pedantic/nursery/cargo lint groups, without suppressions.
- `cargo test --workspace`: 2,332 library tests and 11 binary/integration checks.
- `cargo test --workspace --all-features`: 2,339 library tests and the same 11
  checks. Both retain 19 existing ignored library tests; doctests also passed.
- Asset validation: 593 assets, 370 placeables, eight themes, zero warnings.
- The 362-test source Python run completed with 17 skips and one changelog
  heading failure. Restoring the established dated-heading format fixed it;
  that targeted check passed. No other case failed in that run.
- `cargo build --locked --release --bins` and strict release Clippy passed.
  The preserved compiler reports `places-compile 0.10.0`.
- The real-window empty-install check passed on Metal: no committed world or
  preparation cache, created settings/import/levels, and a captured menu showing
  `v0.10.0`. The player also discovers all eight existing local packages.
- The fresh native compiler displayed actual receiver counts, completed the
  existing `test_room` fixture, cancelled another bake, then completed a retry.
  Its 1,946,139-byte package passed archive validation and
  `verify --require-current`, with identical current/package fingerprints and
  no differences. The compiler window closed and all task-owned jobs exited.

Detailed earlier feature campaigns remain in the linked reports; they are not
represented as a new full-map campaign for this version change. Raw final logs,
native receipts and preserved binaries live in ignored `local/release-0.10.0/`.

## Compatibility and limits

The release follows the repository's annotated numeric tag and source-release
conventions (`0.10.0`), explicitly marked as a GitHub prerelease. There are no
attached platform binaries or compiled maps. Git LFS is required for tracked
binary assets; use `git lfs pull` after cloning.

The compiler executable/version participates in developer currency fingerprints.
Existing matching-catalog packages can remain runtime-compatible while
`verify --require-current` requests recompilation with 0.10.0. Compile selected
authored sources explicitly when current fingerprints are needed. Final release
validation does not rebake all eight production maps or archived maps.

The separately discussed refined Spoonerman delivery is not confirmed installed.
This release retains the previously accepted model (`e0aa64e6c7378d53e1bd08d3aca1c68f1e7beb178a54e7193abf55b232edd87f`);
it does not claim adoption of that later revision. The separate rubber-duck
rebuild and additional Blender work are outside this release.

Native acceptance is macOS/Metal only; Linux/Vulkan and Windows/Direct3D 12,
including their native GUI file pickers, remain unrun. The earlier intermittent
gameplay crashes remain unattributed; bounded diagnostics are retained. Existing
GPU-only ignored tests are not reported as passes.
