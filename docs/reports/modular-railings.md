# Modular railings and fences

The Office shaft, Winter porch/pond fences and Lantern Hollow gardens use
file-backed modular props with shared grounded supports. Straight sections
fit between posts; one capped support closes a corner or terminal. Existing
rooms, walls, floors, water, weather and navigation routes retain their layouts.

`props[].span` fits only the run axis and shears height along it, keeping
balusters upright on slopes. Its continuous collision uses bounded slices
along the same baseline. `material_overrides` replaces selected opaque GLB
material slots from the catalog; snow retains its own material. Preparation
batches compatible instances and shares image allocations. Linking and vertex
fitting do not run per frame. Fitted model UVs clamp; catalog world tiling and
reflection modes do not apply to these overrides.

The timber two-rail section/post are extracted from the existing Outdoors
fence stock, with identical source vertices, UVs, colours and texture pixels.
Winter adds closed, supported snow. The porch kit's old export fitting shifted
the lower rail and distorted corner anchors; corrected exports keep measured
rail elevations and small timber chamfers. No production PNG artwork changed.

The cloud Winter ZIP and preview could not transfer, including one bounded
retry. Those models were **not installed**. This implementation uses existing
repository geometry and source-derived reusable components.

## Local validation

Validation and native acceptance receipts are preserved under
`local/railings-0.10.0/`, outside `target`. The final report records command
outcomes, executable/package identities, native captures and frame costs.

| Gate | Result |
| --- | --- |
| Format; strict all-target/all-feature Clippy, debug and release | Pass |
| `cargo test --workspace` | 2,328 library tests passed, zero failures; binary/integration tests passed |
| `cargo test --workspace --all-features` | 2,335 library tests passed, zero failures; binary/integration tests passed |
| Python source suite | 359 tests, zero failures; four explicit native-only skips |
| Catalog validation | 577 assets, 354 placeables, zero warnings |
| Full read-only asset audit | 350 distinct models, zero integrity errors |
| Texture and prop checks; affected generators; patch whitespace | Pass; texture check retains 96 preferred-size/manifest advisories |

The 19 ignored Rust diagnostics remain explicit native/measurement workflows.
Native acceptance below is separate from those source-test results.

The small added regression fixtures protect shared corner/end supports,
whole-footprint slope grounding, upright span transforms, continuous collision,
schema boundaries, material-slot isolation and compiled image restoration.
They do not bake any shipped map; the package test uses a 2×2 m Static fixture.

Read-only source/GLB review measured 489 post-base samples: every support
contacts its floor, with 6–52.6 mm of base penetration in Office and 6 mm in
Winter/Lantern Hollow. No duplicated post positions were found. Every section
endpoint penetrates a structural post footprint by at least 7.99 mm.

| Authored content | Sections | Shared posts | Drawn kit triangles | Barrier slices |
| --- | ---: | ---: | ---: | ---: |
| Office | 41 | 45 | 11,784 | 223 |
| Winter | 10 | 13 | 1,962 | 66 |
| Lantern Hollow | 24 | 32 | 7,328 | 168 |

Bare fence section/post cost 24/36 triangles; snowy section/post cost 156/102.
Each section is one prop containing its rails/balusters, with no per-slat
entities. Only the three affected active packages are rebuilt; no archived
Demo or Model Zoo package is baked.

## Native acceptance and cost

All three rebuilt packages pass `validate` and `verify --require-current`.
Twenty-seven genuine Metal acceptance runs pass, including the exported Mac
app: Full/Static lighting, flashlight toggles, slope/endpoint/corner views and
eight held-control movement cases. Office reaches both ramp landings and stops
walking/jumping against its rails. Winter keeps its lodge approach clear and
blocks the capped fence post. The garden fence stops the player at its drawn
face. No new floor/wall holes or floating supports were observed on those routes.
These are scripted native-controller checks, not physical-keyboard coverage.

The three unedited 1600×900 captures below show the shared Office corner under
the flashlight, Winter's timber/snow terminal and the supported garden modules.

![Office shared corner under the flashlight](../images/modular-railings/office-shared-corner-flashlight.png)

![Winter timber fence and capped terminal](../images/modular-railings/winter-capped-end-full.png)

![Lantern Hollow linked garden modules](../images/modular-railings/outdoor-linked-fence-full.png)

Fixed-camera ABBA measurements use the pre-change player/packages and the final
player/packages on the same M2 Pro/Metal host. Each side has two 300-frame runs,
60 warmup frames per run, High textures, Full lighting, reflections Off, bloom
On and a 1600×900 drawable. Builds finished before measurement. GPU samples are
deduplicated; explicit completion waits make these comparisons unsuitable as
ordinary gameplay FPS claims.

| View | Base-scene draws, before → after | Level vertices, before → after | Median GPU ms, before → after |
| --- | ---: | ---: | ---: |
| Office slope | 59 → 61 | 53,172 → 83,796 | 0.720 → 0.791 |
| Winter fence | 30 → 32 | 336,408 → 337,142 | 1.841 → 1.356 |
| Garden fence | 305 → 306 | 738,248 → 745,010 | 1.526 → 1.511 |

Material changes per sampled frame are 27→29, 23→26 and 204→205 respectively.
Office's VBO grows from 4.46 to 7.03 MiB. Its Full atlas grows from 144 to
192 MiB: one additional page across the base and two switched-light states.
Winter and Lantern Hollow retain their 96/160 MiB Full atlas allocations.
The matched stationary Office update median is 1.566→1.567 ms. The garden GPU
difference is within run spread; the other values describe these particular
changed views, rather than a general engine speedup. Machine receipts retain
all samples, hashes, source/model/atlas inventories and package/native identity
checks under `local/railings-0.10.0/`.

The preserved standalone export contains the three refreshed maps. The five
untouched repo-local packages retain their old catalog provenance and need
rebuilding before use with this new catalog; their files were preserved as
requested. Packages, binaries and full campaign logs remain outside Git.
