# Runtime lighting receivers — macOS, 2026-11-04

Three lighting-audit findings were reproduced and corrected from clean remote
`0.13.1-dev` at `fc0852944620803fc6023e04f536d6115131d6cf`. Work used an
independent clone in task-16; the integration and audit checkouts were preserved.
No release, main merge, force push, bake precision reduction or shipped-map
rebuild is part of this change.

## Behavior

Contact footprints retain the eight-slot projected-bounds approximation. Their
support planes now follow ramps and their own storey. The shared material shader
rejects receivers off that plane, clips the support-to-receiver segment against
opaque static world triangles, checks intermediate floor support, and bounds the
negligible Gaussian tail before tracing. An internal wall works independently
of room identity; a real opening still admits a footprint.

Selection samples five potential floor receivers per candidate, rejects
out-of-frustum/occluded footprints and ranks the remainder by nearest visible
receiver distance. Geometry provides deterministic ties independent of spawn
order. Objects and characters share the same ranking. The selected uniforms are
resolved once per displayed frame and reused by its material passes.

Cabin lights retain the exact existing shell, sill, both door tracks, moving
aperture and centre-seal tests. A subsequent finite static-world segment query
adds hall blockers. The world/decal/reflection shaders share this data and policy.
The immutable transport hierarchy is uploaded once per installed build, with
no lighting solve, new shadow-map pass or extra scene draw. Static installs
without an existing transport scene prepare geometry only.

The GPU snapshot stores 48 bytes per hierarchy node and per ray triangle, using
the existing hierarchy/leaf order. Traversal is stackless, capped at 512 visited
nodes and 64 triangles per leaf. Unavailable/oversized geometry and exhausted
queries reject transmission. Alpha-tested/blended and transmissive faces are
excluded from this opaque-only snapshot; this change does not provide general
shadows from moving objects. Contacts remain an approximation with a support
plane and a midpoint support check, not a mesh silhouette or arbitrary floor
connectivity solver. No lightmap values, density, HDR storage or output precision
were reduced.

## Native evidence

Direct all-feature release player, Apple M2 Pro / Metal, High graphics,
1280×720 physical drawable (640×360 logical window), standard style, bloom and
reflections off. Each run preserves player/package/source identities, PNGs,
settings, actions, logs, frame CSV and capture diagnostics. Static means the
`off` lightmap representation, retaining its normal ambient field.

| Witness | Result |
| --- | --- |
| Original solid-divider contact, Full and Static | Hidden-crate insertion/removal changes zero pixels; eight remote hidden crates leave the local-shadow image byte-identical |
| Same-room internal solid divider, Full and Static | Same zero-pixel results; room membership cannot bypass the wall |
| Visible caster behind 0.4 m internal wall | Caster remains selected; 20,760 floor pixels across the wall remain identical in both modes |
| Open passage with solid flanks | Real footprint reaches through the opening; 17,907 floor pixels whose segments cross solid flanks remain identical in both modes |
| Same-side contact positive control | Full: 3,314 of 4,233 isolated floor-ring pixels darken, maximum 5/255; Static: 3,298 pixels, maximum 5/255 |
| Ramp | Contact follows the sloped receiver: Full isolated ring has 2,175 changed pixels, maximum darkening 4/255; Static also retains a visible contact |
| Stacked floors | Lower and upper native views retain their own local contacts; lower caster insertion changes zero upper-view pixels in both modes; a lit Full upper-floor ring retains contact darkening; CPU regression also checks both storeys on a slope |
| Nearby mannequin plus eight hidden crates | Character receives a slot; selected footprint remains the same through the small camera move |
| Opaque 40 mm hall screen, Full | All 47,968 floor witness pixels remain black when partly/fully open, at 0° and 90° |
| Same screen, Static | Opening changes zero witness pixels at both rotations; normal ambient remains |
| Unobstructed Full control | 0° open: 42,585 witness pixels light, maximum 29/255; 90°: 42,868 pixels, maximum 27/255 |
| Partial opening positive control | 0° Full: 12,580 witness pixels light while the authoritative opening is approximately 0.25; closed witness is black |
| Rotated Static positive control | Opening increases 20,260 witness pixels; screened equivalent changes zero |

Floor witnesses use camera-ray/plane intersections with explicit world bounds.
The cabin region is local X −0.65…0.65, Z −0.75…−0.35 at Y=0; a fixed 48×48
centre box excludes the UI crosshair. It differs slightly from the original
audit's 49,563-pixel mask, so counts are not presented as identical-mask deltas.
The low-wall control keeps the caster selected, independently proving fragment
clipping rather than relying only on candidate rejection.

The existing 31-view elevator acceptance additionally checks inside/header,
side/rear shell, four floor corners, partial opening/closing, sealed departure
and upper arrival in Full and Static. Both 31-view runs pass all built-in seal/illumination assertions. Full open
front-floor mean red is 7.675/255 versus exactly zero when closed; Static
closed equals its source-out-of-range reference. Exact results are retained
with the acceptance receipts.

## Frame cost

Matched baseline/current direct executables, same packages/views and settings,
V-sync off, actual transition to Immediate confirmed, 1280×720 drawable. Contact
runs discard 300 warmup frames and measure 600; the cabin table uses the final
600 stable open frames. No frame limiter is active. Mean loop cadence is
approximately 162–202 fps, above the 120 Hz display rate, rather than plateauing
at refresh. Timestamp spans include GPU scheduling stalls.

| State | Baseline GPU mean ms | Corrected GPU mean ms | Baseline / corrected loop mean ms | Scene draws |
| --- | ---: | ---: | ---: | ---: |
| No contact subjects | 1.112 | 1.390 | 5.422 / 5.494 | 6 |
| One visible crate | 1.145 | 1.575 | 5.551 / 5.362 | 7 |
| Eight visible contacts | 1.618 | 2.570 | 5.329 / 4.939 | 14 |
| Eight hidden crates plus local crate | 1.408 | 1.656 | 6.166 / 5.444 | 7 |
| Open cabin, unobstructed hall floor | 1.571 | 3.227 | 5.250 / 5.903 | 12 |

The eight-contact and cabin witnesses add about 0.95 ms and 1.66 ms GPU time on
this fixture/hardware. These are correctness costs, not a throughput improvement
claim. Loop/presentation tails are noisy on the shared host. The measurements do
not justify increasing the slot budget or predict production-map/other-GPU cost.
There are no additional scene passes or draws.

Baseline player SHA-256:
`a41bd4c9d3aa31d8dc189223bda75308e04d498955a8bd14dc1650455fb0cd5b`.
Corrected player SHA-256:
`44672ae00ea3494093520c2c09406d68aa81796bfe3e6ae858b228f35c31f8f8`.
Compiler SHA-256:
`57499f0e06f23fe25e7a80c980a09f1e2a4d6528966c3a847231bc454c2146e3`.
Tested Rust/WGSL source identity (sorted relative path, NUL, bytes):
`157cf8b9ef58c6558b62eeb71bc933dca20f45ee36e1511a784f9d4716787ca2`.

## Source gates and limits

Rust/Cargo 1.99.1; format check, strict workspace/all-target/all-feature Clippy,
locked workspace/all-feature release build and all **468 workspace tests** pass.
Tests use `PLACES_ASSET_ROOT=<checkout>/assets`; omitting it in the isolated target
initially caused three asset-discovery failures, resolved by the documented
resource configuration. Two new asset-free CPU regressions cover slope/storey
planes and hidden insertion/stable eight-slot selection.

Asset validation passes with the same 17 existing placement warnings. The Python
source suite runs 373 tests: **353 pass, 16 skip, four fail**. All four failures
reproduce on the untouched base checkout: Lantern Hollow return links and tree
boundary count, collection hub transition links, and the Arcade party-room
baseboard/walking route. Their authored maps and tests were not changed here.
These pre-existing source-contract failures remain for their owning work.

No baked banding defect was established by the original audit. Its coplanar
control and unique evidence remain in task-10. This campaign does not claim
coverage of other backends, every production map, arbitrary floor gaps, or
moving/alpha-masked occluders. Static/Full, shell and ordinary aperture behavior
were inspected on the dedicated fixtures only.

## Reproduction and retained files

The opt-in runner creates only a small dedicated source and its Static/Full
package. It requires the shared `/tmp/places-audit-validation.lock` to be owned
by `audit: lighting` and refuses an existing output directory:

```sh
python3 tools/bench/lighting_receivers.py \
  --binary <direct-release-player> --compiler <direct-release-compiler> \
  --out <fresh-evidence-directory> --case internal --quality full
```

Cases: `contact`, `internal`, `low-wall`, `doorway`, `slope`, `stacked`, `screen`,
`clear`, `screen-rotated`, `clear-rotated`. Repeat with `--quality off` for Static.
Use `--case stacked --upper` for the lit upper-storey view. The contact route
also spawns a nearby mannequin and moves the camera. Screen
routes retain closed/partial/open checkpoints and captures. Existing
`tools/bench/elevator_light.py` provides the extended 31-view seal check.

Unique receipts and images are retained outside the checkout at
`/Users/connordawkins/Documents/Codex/2026-11-04/task-16/evidence/`:
`pixel-results.json`, `contact-regions.json`, `performance-summary.json`,
`repro-character/`, `contact-*`, screen/control runs, `seal-full/` and
`seal-static/`. Raw comparison runners and CSVs are there too. The isolated
validation target remains available for the next task; no other task's files
or processes were removed.
