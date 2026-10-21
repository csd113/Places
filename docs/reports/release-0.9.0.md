# Places 0.9.0 — finalized release preparation

October 13, 2026. Places is a game for walking and experiencing places;
custom game modes are coming. This release integrates the completed exploration,
content, compiler and reliability work on `arcade-env`.

## Meaningful changes

- Eight supported environments: Pool, Home, Lantern Hollow, Winter, Beach,
  Frutiger Aero, Cosmic Lanes & Arcade, and Office: After Hours. Arcade includes
  its birthday room, ultraviolet materials and connected restroom; Office
  connects three floors with sparse basement encounters.
- Engine builds, ordinary tests, CI, default exports and startup support zero
  installed maps. Packages are external local content, with no mandatory map
  download or embedded compiled world. Demo and Model Zoo remain historical
  JSON only in `docs/archive/levels/`, outside active discovery and baking.
- Compact prepared-map storage, independent Static/Full lighting and texture
  controls, shared capture resources, atlas packing and lighting/fixture repairs
  are integrated. The offline compiler reports terminal progress.
- Shared ramp/stair contacts repair the reproduced Office pressed-wall camera
  penetration and jump escape; validated-spawn recovery handles sustained escape.
  Office rail mounts, ceiling lighting and basement reach are corrected.
- Drawable bounds and SDL-window ownership are hardened; bounded persistent
  diagnostics and readable failure notices retain useful session evidence.
- The promotional README uses the approved existing artwork as `logo.png`,
  four current native screenshots and an eight-environment walking montage.
  Build and installation guidance lives in the developer documentation.

## Final acceptance

| Check | Result |
| --- | --- |
| Fresh release player/compiler build | Passed; version 0.9.0 |
| Formatting, all-target/all-feature check, strict debug and release Clippy | Passed without lint changes |
| `cargo test --workspace` | 2,263 library + nine integration tests passed; library 32.17 s |
| All-feature workspace tests | 2,269 library + nine integration tests passed; library 32.65 s |
| Map-free offline all-target/all-feature source check | Passed with production map directories and compiled packages physically absent |
| Authored asset/generator checks | Passed; no asset validation errors or warnings |
| Eight installed packages | Refreshed with 0.9.0; current-input/archive checks, Static/Full atlases and independent player discovery passed |
| Native package compatibility | All eight loaded and presented Full-lighting worlds on M2 Pro/Metal; no skipped ready presentations |
| Office pressed-jump controls | East remained at or beyond X34.3; west at or below X30.7; eye height stayed at or above 1.6 m |
| Empty export | Zero packages; launched from outside the checkout, created settings/import/levels, committed no world and created no preparation cache |
| Native notice and empty-menu interaction | Controlled interrupted-marker notice was readable and dismissible; Settings, return and menu Exit passed with exit 0 and marker cleanup |
| Explicit Low-lighting GPU resource controls | Both passed in 23.34 s |

The first `tools/verify.sh` invocation passed its build, content, package and Rust
stages, then found three stale native-harness assumptions among 388 Python tests.
Installed production maps could select the fixture background, and two assertions
still expected texture quality to control lighting. The harness now shares an
asset-only fixture payload and tests independent lighting/texture behavior.
The entire affected 25-test native class passed in 118.26 s; the remaining desktop
GPU stages were completed separately. No production Rust, shader, map or artwork
was changed for that repair, and no additional test cases or full-map Rust bakes
were added. The original failed invocation is retained as a private receipt,
rather than represented as an uninterrupted successful run.

## Native showcase

[Download the walkthrough](../videos/places-0.9.0.mp4?raw=true): **32 seconds**,
960×540, H.264/YUV420P, 30 fps, approximately **3.47 MiB**, with eight chapters
and simple cuts. It contains consecutive native frames from real held player
controls in each current package: Office → Pool → Home → Lantern Hollow → Winter
→ Beach → Frutiger Aero → Arcade. Each segment records 4.16–5.20 m of actual
player travel. The render eye follows the player; no independent flythrough,
synthetic scene, still-image slideshow or cloud model candidate is used.

Recording uses the existing native frame-sequence tool with a fixed simulation
step; its output frame rate is **not a gameplay performance measurement**.
There is no audio track. The eight-second animated lossless WebP samples moving
footage from every environment at 384×216/6 fps and is approximately 2.96 MiB.
It is encoded directly from the original native RGB frames; all 48 decoded
frames match their downsampled sources exactly, preserving the full colour range.
The thumbnail reuses the native Arcade PNG. Four unaltered 1280×720 still frames are retained in
`docs/images/release-0.9.0/`; raw sequences and intermediate clips are disposable
after publication validation. GitHub renders the animated WebP; its file viewer
does not show this MP4 inline, so the README links directly to the raw download. Public
download bytes and exact staging/main CI identities are verified in the handoff.

## Honest limitations and preservation

**The original intermittent gameplay crashes remain unattributed.** The
[crash investigation](release-0.8.0.md#crash-diagnostics-investigation) establishes the renderer
invariant fixes and bounded evidence, not elimination of those crashes. This
release completes the previously inconclusive notice visual check. Short native
walks and fixture controls do not establish exhaustive traversal or long-session
stability, and no device-loss injection was performed.

Current native acceptance is macOS/Metal. Linux/Vulkan and Windows/Direct3D 12
were not exercised in this campaign. Existing sub-texel charts can remain
vertex-lit within otherwise valid Full atlases; the package checks retain their
Full-atlas requirements. Separate Blender/GIMP model candidates remain outside
production, and the GPU bake feasibility proposal is not implemented here.

Compiled `.placesmap` packages stay ignored and outside GitHub. The eight playable
packages and a local playable macOS export are retained outside `target`.
Unrelated user audit files are preserved. Only this task's reproducible outputs
are removed; target cleanup and custody release follow completed publication/CI.
No tag or GitHub Release is created.
