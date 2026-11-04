# Elevator cabin-light containment — macOS

2026-11-04, Apple M2 Pro / Metal. Direct release executables only. Validation
includes the final flush doorway and cabin floor-skirt repair from `6e9d328f`,
the current hub elevators, independent door control and shared quick-play audio.
The separate editor-placement change is outside this lighting acceptance.

This report identifies the lighting worker's retained build before stationary
ride integration. The later [fixed-cabin acceptance](elevator-stationary-ride-0.13.1-mac.md)
keeps a separate enclosure and landing receiver for every fixed cabin. The
measurements below remain specific to the exact player/packages identified here.

## Reproduced faults and correction

The cabin published an unrestricted attached point light. Opaque elevator meshes
did not shadow that bounded additive path, so light crossed closed doors and the
shell. A source-local enclosure now follows the authoritative car pose, actual
eased leaf overlap, both tracks, fixed pockets/jambs, header, sill and shell.
The shared world/decal/reflection shader clips the cabin source in Static and
Full. The practical stays on inside a closed or moving car. There is no extra
draw, shadow-map pass, runtime bake or global light-strength change.

After closing that spill, the door panels still looked gray against a black
hall. Both leaf and frame GLBs have zero emissive factor and no emissive texture.
Their geometric centres occupy wall/shaft stock, outside valid room air, so the
receiver path supplied a roomless 0.1 ambient fallback. Panel pixels stayed
identical with the cabin present and with it upstairs beyond the source radius.

Factory assemblies now select real adjacent landing air for environmental
lighting. Full retains actual receiver/source visibility and uses zero
unsupported bounce. Leaf samples cover exposed steel rather than the concealed
sill/header overlaps. The final 20 mm skin has render-specific ±.010 m faces;
lighting uses those faces independently of the wider conservative collider.
Frames finish at hall Z=0 and cabin Z=.360. The car keeps its departure receiver
until arrival, and new assemblies receive real lighting before their first GPU
upload. Materials, emission, global ambient and unrelated objects are unchanged.

The shader skips enclosure work when the surface's cosine light contribution is
already zero. This preserves the light result and avoids tracing stock behind
back-facing landing walls.

## Native results

Each fixture run records 31 actual-game PNGs and authoritative elevator-state
checkpoints: closed/partial/open/closing, both oblique hall angles, cabin/header,
shell side/rear, all four floor corners, and upper-floor arrival. All four runs
pass: Full, Static, bloom enabled and the positive cool hall-light control.
Full and lit control make 558 uncached-versus-cached receiver comparisons with
zero mismatches; capture logs also retain actual uploaded lighting values.

Values below are display RGB on a 0–255 scale, not radiometric units.

| Sample | Result |
| --- | --- |
| Previous bright closed left panel, dark hall | 50.92 / 51.81 / 47.80 mean |
| Final closed left/right panels and sampled frame left/top | 0 / 0 / 0 mean; maximum channel 0 |
| Final closed hall floor below sill | 0.000/0.000/0.000 |
| Same floor, early opening | 2.647/0.981/0.014 |
| Same floor, fully open | 7.675/2.675/0.029 |
| Same floor, partially closing | 2.360/0.873/0.012 |
| Same floor after seal | 0.000/0.000/0.000 |
| Final closed left panel with real cool hall lamp | 31.48 / 40.43 / 48.00 mean |

Closed surfaces/floor match the source-out-of-range reference exactly. Side and
rear floor witnesses remain black while the doors open. Inside, the practical,
diffuser, inner doors, walnut, rails and ceiling remain lit. All four floor-corner
views show the final continuous skirting without the previous blue/slate gap.
Static retains ordinary ambient throughout the hall: closed and source-absent
floor RGB match exactly at 37.734 / 30.488 / 17.944. Bloom retains zero closed
panel/frame samples.

The final-art travel/loading/save/resume/return campaign passes in 28.5 seconds,
using the doorway task's already prepared lab package. No other map was baked.
The clean demonstration records 369 native 1280×720 frames over
7.418 seconds, preserving source timing in a 30 fps MP4. Ordinary
mouse controls open the elevator, close it and send it upstairs. The camera is
fixed for comparison. No generated imagery or overlays are added.

## Geometry and shader checks

- 446 workspace tests, strict all-target/all-feature Clippy, formatting and
  locked all-feature workspace release build pass.
- Actual-GLB doorway sightlines: 21,546 closed rays, zero leaks. The corner audit
  checks 302 shell and 27 floor-join rays, zero leaks. Frame-fit audit passes.
- Production WGSL on Metal: 95,550 samples over seven opening states and five
  rotations; 9,500 closed-front and 1,890 shell rays blocked, 825 clear fully-open
  rays, zero blocked transmissions to eligible receivers. The triangle oracle
  allows 0.1 mm receiver contact tolerance. It explicitly excludes 155 synthetic
  samples inside concealed solid centre seals, which are not visible receivers;
  ordinary opaque depth hides that stock. All 560 interior panel/control/walnut/
  header samples remain lit.
- Asset validation passes with 17 existing collection-wall placement advisories
  from the hub work. This lighting change does not alter those authored maps.

## Frame cost and limits

Two matched states use the same final art/package, view, High/Full settings,
bloom/reflections, 1280×720 drawable, 120 warmup and 600 measured frames.
V-sync is disabled and actual Immediate presentation is confirmed. Every run
presents 720 ready frames with zero missed acquisition, and retains 14 base-scene
draws. The baseline is the integrated pre-light-fix player
`5589b59009c0e18f3ac102f2740e46907f25f7eb5a5622f7fd52fdfabf6f5844`.

| State | Baseline GPU mean ms | Corrected GPU mean ms | Baseline/corrected loop mean ms |
| --- | ---: | ---: | ---: |
| Closed | 1.425 | 1.477 | 4.529 / 4.714 |
| Open | 1.410 | 1.627 | 4.696 / 4.571 |

Timing uses the earlier final-doorway player
`3630ccdfc9ce021c0163b4552ad6f62cfcb5a76699581161fb9f6948df203049`; its renderer/art
objects are identical to the combined development version.
The local GPU cost is approximately +0.05 ms closed and +0.22 ms open. Loop tails
are noisy on the shared host; these are bounded fixture measurements, not a
whole-game FPS claim. Timestamp spans include scheduling stalls. Before the
zero-cosine shortcut, repeated open-state pairs added about 0.47 ms. No extra
render pass or draw is introduced. Other GPUs/platforms are not established.

The enclosure follows the manufactured shell and rigid door stock. Future mesh
changes must update and recheck that contract. It does not add general-purpose
point-light shadows for unrelated meshes. Existing accepted packages with
matching assets receive the runtime fix; normal source-current verification may
require refreshed preparation provenance. Only the two dedicated lighting
fixtures were prepared here.

## Reproduction and retained evidence

```sh
places-compile build tests/fixtures/levels/elevator_dark.json \
  --out <dark.placesmap> --variants static,full
places-compile build tests/fixtures/levels/elevator_dark_lit.json \
  --out <lit.placesmap> --variants static,full
python3 tools/bench/elevator_light.py --binary <direct-player> \
  --package <dark.placesmap> --out <new-directory> --assert-sealed --trace-entities
python3 tools/bench/elevator_light.py --binary <direct-player> \
  --package <lit.placesmap> --level elevator_dark_lit --expect-hall-lit \
  --out <new-directory> --assert-sealed
```

Repeat the dark command with `--lightmaps off` and `--bloom`. These are opt-in
manual tools; normal tests and CI never bake or open a game window.

Local evidence is retained outside `target`, under ignored `evidence/`:
`development-final-full`, `development-final-lit`, `development-final-static`, `development-final-bloom` contain captures,
checkpoints, measurements, logs and receipts; `development-final-video/elevator-dark-room.mp4`
is the native demo; `development-final-transport/flow` retains the lab and transport proof;
`performance-doorway` contains presentation/identity and raw frame/GPU receipts.
`development-integrated` retains the exact direct player and compiler. Historical
`receiver-*` captures predate the final doorway and are superseded.

Final player SHA-256: `a22e1eefe8a35a000c1a53564c3949b29ca0b5bfc276b5f8ba49bae7ff9a8f47`.
Dark package SHA-256: `27efa2892cedd09234f6730d44f09744146244cff41dbe244ce0fe39399b0fe7`.
Lit package SHA-256: `1e96b5dc5fd7eeb5ff337bb41fce7ffc5f225a6b4e634325aadea176cb6fabcd`.
Both packages pass current-source verification. The validated source includes
`6e9d328f` art and the lighting integration based on `9a771f1b`; the development
merge also preserves predator-search commit `3fee560e`. Git object checks confirm
that renderer, elevator code, final art and fixture sources remain byte-identical
to the original lighting acceptance. Both current-source packages and all native
runs were refreshed with the combined release executable.

The explicit capture tool now separates aim, mouse press and mouse release by
checkpoint boundaries. Short 70 ms clicks could be delivered and released in one
delayed update under shared-host/PNG load. The failed lit run and supplementary
old-player run are retained but excluded from accepted open/travel/reference
results; the revised tool keeps every state assertion and all final runs pass.
Only the supplementary old player's valid initial closed capture is used for
the matching final-art before image.

Existing hub exports need refreshing after the final GLB dependency changes;
their stale-asset guard remains intact. Hub/collection export is owned by the
parent hub task. This campaign prepares only the two dedicated lighting fixtures
and reuses the doorway task's accepted lab for transport. No release or main
merge is performed.
