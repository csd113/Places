# Office concept-art refinement

Historical acceptance: October 2026 Office refinement. Counts, timings and validation below describe
that tested version; the canonical guides govern current contracts.

The existing Office assets now have separately readable furniture construction,
stronger material artwork and remade wallpaper. Asset IDs, material settings,
collision footprints, map sources and the rendering architecture are preserved.
The reference is `assets/environment/office/Liminal Office Worlds Asset Board.png`.

## Reference analysis and inventory

The board's main cues are faded vertical yellow wallpaper, dense beige carpet,
acoustic tiles with a regular suspended grid, bright tubes inside metal frames,
wood laminate over grey metal furniture, charcoal fabric, blue water bottles
and a mountain illustration on the drinks machine. Small seams, handles, inset
panels and narrow edge treatments communicate construction. Water damage is
irregular, while maintained surfaces stay quiet. Large empty spaces and sparse
furniture preserve the liminal atmosphere.

All five Office GLBs, their embedded/native atlases, six tiling surface sheets,
one fitted fluorescent face and seven Office materials were inspected. The
automatic baseboard, ceiling grid and fluorescent housing were reviewed in the
renderer, together with the demo's shared door/frame, window and vent details.
There are no registered Office computers, monitors, keyboards, cubicles or
signs in the existing set. Generic demo furnishings remain outside this pass.

| Existing asset | Assessment and action |
| --- | --- |
| Clean wallpaper | Remade at the user's request: vertical pinstripes and staggered faded lozenges replace double chevrons. Neutral cream albedo retains the existing yellow material tint. |
| Stained wallpaper | Derived from the new print; restrained water runs, rubbed paper and diffuse tide marks. Seam repaired and tested. |
| Clean beige carpet | Already has convincing dense short pile, warm tone and subtle variation; retained. |
| Damp carpet | Replaced uniform-looking damage with organically soaked, flattened patches and soft edges; retained fibre scale and repaired wrapped seams. |
| Clean acoustic ceiling | Existing pores, panel variation and grid are readable and consistent with the board; retained. |
| Stained acoustic ceiling | Reworked water damage inside the existing 2x2 grid; preserved seam orientation and acoustic texture. |
| Fluorescent face | Refined twin tubes, sockets and frosted diffuser ribs at the existing 2:1 aspect. Existing recessed housing and frame retained. |
| Baseboard and ceiling grid | Existing geometric trim, corner treatment and alignment are adequate. Baseboard inherits the remade wallpaper through its existing darker material tint. |
| Shared door/frame, window and vent | Existing depth, frame and louvre construction reviewed in demo captures; retained without changing shared assets. |

## Furniture and texture changes

| Model | Before triangles | After triangles | Refinement |
| --- | ---: | ---: | --- |
| Desk | 142 | 356 | Narrow desktop bevel, warm wood grain, metal pedestals, separated drawers, bridge pulls, pencil drawer and modesty panel. |
| Chair | 704 | 704 | Existing shaped/caster geometry and repairs retained exactly; new woven charcoal upholstery and restrained plastic/metal swatches. |
| Cabinet | 354 | 212 | Rebuilt as two broad metal filing drawers, recessed plinth, label holders and bridge pulls within the original low cabinet footprint. |
| Water cooler | 206 | 556 | Open recess, separate red/blue controls, drip tray and continuous eight-sided ribbed bottle; off-white cabinet and blue plastic artwork. |
| Vending machine | 146 | 252 | Inset mountain/DRINKS fascia, separate frame, raised selection buttons, coin bezel and delivery lip. |
| **Total** | **1,552** | **2,080** | **528 additional triangles; all models below the existing 800-triangle review budget.** |

Five new committed 1024-square prop master PNGs derive the existing 256-square
native atlases with the established box filter. The GLBs embed only the native
atlases. There are no new gameplay assets, material IDs, runtime texture sources
or external dependencies. ImageGen supplied the offline artwork; processing uses existing resizing and seam
repair tools; no runtime procedural imagery was introduced.

The Office surface and fluorescent exporters now load the authoritative PNGs,
including forced builds. This prevents the historical low-resolution painters
from replacing refined artwork. Tiny routing edits in the shared furniture,
appliance and fixture authoring modules select Office-only builders. The asset
specification's wallpaper description is updated to match the new print. The
stained wallpaper's existing seam-tool preset uses its new repair parameters.
These shared edits are necessary for repeatable authoring; catalog and Rust changes
belonging to the concurrent Winter work are excluded. A narrow Rust test
expectation update for the reduced cabinet chart count is explained below.

## Actual renderer review

The dedicated `tests/fixtures/levels/office_asset_review.json` places all five
props in maintained architecture beside a damaged room. It is a validation
fixture, not an additional shipped map. `tools/bench/capture_office.py` records
eight fixed views through the actual wgpu Metal renderer: wide, desk/chair,
filing cabinet, cooler, vending, ceiling, transition and damaged room. The same
tool records seven demo views: reception, workroom, desk, hallway, ceiling,
window and vent. Settings and camera positions are identical before/after;
captures are 1280x720 on Apple M2 Pro.

Native High and Low fixture/demo comparisons used one fixed renderer, camera
and settings. Original/refined sources, matching packages/assets and the saved
player are independent reproduction inputs. The current combined package result
is recorded in the [October 7 integration](integration-provenance-20261007.md).

The largest differences are the quieter vertical wallpaper print, convincing
metal/wood desk construction, visible cabinet drawer reveals and label plates,
open cooler recess and ribbed bottle, and the fitted illustrated vending face.
The damaged room has clearer differences among water-stained paper, soaked pile
and discoloured ceiling panels. Tube/socket detail survives gameplay views.
Empty floor area and repetition remain intact.

Renderer inspection caught a cabinet carcass hiding its fitted drawer faces;
the carcass was shortened behind the fronts and recaptured. Geometry inspection
also caught coincident component endpoints in the cooler and vending frame;
those joints were adjusted and revalidated. Final prop topology has no
degenerate faces, duplicate faces, zero-area UV faces, open boundary edges,
inconsistent winding, flipped triangles, nonmanifold edges or contradictory
components. Closed assembled pieces deliberately penetrate their supports.

## Validation

The four isolated shipped packages contain Off, Medium and Full variants and
pass both `places-compile validate` and `verify --require-current`. Asset
validation reports 287 assets / 165 placeables / five themes with zero warnings.
Texture and prop build checks pass; the texture checker reports the existing
56 soft size/manifest warnings. All five master/native atlas checks pass.
The full asset audit reports zero integrity errors across 165 models.

All six tiling sheets pass `tools/textures/seam_repair.py --check`.
`places --check-geometry --level tests/fixtures/levels/office_asset_review.json`
reports zero errors and zero warnings. The native fixture and demo camera
matrices pass on High and Low with no failed GLB loads.

`python3 -m unittest tests.test_office_assets tests.test_package tests.test_asset_audit`
ran 58 cases: 57 passed and the package case encountered unhydrated LFS pointers
in the newly created worktree. `git lfs checkout` hydrated them; the bundled
package integrity case then passed, including a final rerun after all bakes.
No test was skipped. Prop re-exports reproduce all five GLBs byte for byte.

The full Rust suite initially passed 2,006 tests and found one authored-fixture
count assumption: the refined cabinet reduces Hollow to 229,549 lightmap charts,
below a hard-coded 230,000 floor. The density guard now requires 225,000, while
the regression's actual checks remain unchanged: metadata exceeds the old
materials cap, the old-cap reader rejects it, the dedicated cap loads it,
eight atlas pages remain, and real stump/boulder vertices retain atlas UVs.
This is a test expectation update; no runtime budget or limit was changed.
The corrected focused test passed. The next complete run passed all 2,007
library tests but its cwd-discovery integration case exposed an incorrect
validation override: `PLACES_ASSET_ROOT` must name the repository root, not
its `assets` directory. Pointing it at the fixed repository made all three
launch-layout tests pass; no CLI or asset-discovery source changed.

Final Rust commands used the fixed source/asset snapshot, `PLACES_ASSET_ROOT` pointing at
that repository root, a private cargo target and uncached
compilation (`RUSTC_WRAPPER=`, `CARGO_INCREMENTAL=0`) to avoid stale embedded
source/package data discovered during concurrent testing:

* `cargo fmt --all --check` — passed.
* `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` — passed.
* `cargo test --workspace --all-features` — passed: 2,007 library tests plus
  all binary and integration tests; 23 pre-existing ignored library cases.
  The final complete run exited 0.

Asset and visual commands completed successfully:

* `python3 tools/assets/validate.py`.
* `python3 tools/textures/build.py --check`.
* `python3 tools/props/build.py --check`.
* `python3 tools/props/build_office_textures.py --check`.
* `python3 tools/assets/audit.py` (full inventory and integrity checks).
* `python3 tools/textures/seam_repair.py --check <the six Office surface PNGs>`.
* `places --check-geometry --level tests/fixtures/levels/office_asset_review.json`.
* `places-compile build`, `validate` and `verify --require-current` for all
  four dependent packages, plus the compiled review fixture.
* `python3 tools/bench/capture_office.py` with fixed High/Low settings for
  the review fixture and demo; twelve paired native benchmark runs.

## Performance evidence

The five native atlases retain **1,280 KiB** of uncompressed texture residency,
five prop draws and one material/atlas per model. No high-resolution master is
loaded by the game. Surface dimensions, texture count and mip-chain costs are
unchanged. Each model stays below the existing review/hard budgets. Total GLB
file bytes rise from 373,564 to 577,008 as the richer atlases compress less;
Office surface PNG bytes fall from 11,077,249 to 10,265,527. Five retained masters
are authoring sources only. A read-only re-export reproduces all five final
GLBs byte for byte, including the chair geometry-preserving path.

In the fixture's wide High view, total static batches remain 21, visible draws
remain 13 and texture binds remain 10. Vertex buffer bytes change from 336,640
to 438,016 and index bytes from 11,220 to 14,388. Full lighting charts change
from 1,607 to 2,135, with two atlas pages, 32 MiB lightmap residency and 132
irradiance probes in both versions. The all-variant fixture package changes
from 2,741,125 to 2,846,960 bytes (+3.9%); compilation measured 10.33 s before
and 10.13 s after. These wall times were gathered during concurrent work and
are indicative, not a controlled speed comparison. Capture frame timings are
also short, concurrent samples; geometry, batches and residency are the more
reliable comparison.

A paired native benchmark alternated before/after roots for three runs per
quality with one pinned release binary, 60 warmup frames and 600 measured frames
per run. Median-of-run mean render time was 4.785 ms before / 5.030 ms after on
High and 4.951 ms before / 4.819 ms after on Low. High before samples ranged
from 1.580 to 5.256 ms, demonstrating substantial background/display variability;
this does not establish a precise performance delta. The cost counters are
unchanged except the documented vertices and indices.

The unchanged-source demo bake produced 96,219,764 bytes versus 96,163,308
before (+0.059%). Medium/Full charts increased from 153,410 to 154,332 (+0.6%);
prop batches remained 151, Full atlas pages remained eight, and navigation,
collision and world mesh counts were unchanged. Wall-clock bake time grew from
521.9 s to 958.2 s during heavy concurrent builds; the small chart increase does
not explain that timing increase, so it is not attributed to the assets.

## Remaining gaps and shared findings

The concept's tall filing storage was interpreted within the existing low
cabinet bounds to preserve placement and collision compatibility. Computers,
desktop clutter and additional signage would be new gameplay assets; they were
not needed to improve the existing set and were not added. The chair already
had stronger authored geometry than its primitive generator and was preserved.

Fixture face UVs use the existing renderer orientation, so rotated housings do
not provide an independently authored face rotation. Very bright emissive
diffusers suppress fine rib detail at distance. The existing lighting paths
also shade assembled prop faces differently between Low and High; the reviewed
assets remain readable on both. Sub-texel faces use the existing vertex-lighting
fallback rather than receiving their own lightmap chart. No global lighting, shader, geometry format or
asset limit was changed to address these constraints.

The compiler fingerprints the entire catalog even when new registrations are
unrelated to a map. Concurrent catalog edits therefore invalidate otherwise
successful asset-only bakes. Final asset comparisons used fixed source/asset
snapshots; combined-state integration requires matching package identities.

Four bundled packages reference changed Office assets: `places_demo`,
`movement_test`, `lantern_hollow` and `model_zoo`. Their package outputs require
rebuilding even though their source maps are unchanged, so runtime freshness
checks accept the new assets. Winter has no Office dependencies and its files
are excluded from this pass. Mixed-theme maps that already use these Office
props inherit their revised appearance; existing placement and collision bounds
remain compatible.

## Files changed

* `assets/environment/office/README.md`; five Office prop GLBs, five native PNGs
  and five new master PNGs under `assets/environment/office/props/models/`.
* Five Office surface/fixture PNGs: both wallpapers, damp carpet, stained
  ceiling and fluorescent face. Clean carpet and clean ceiling remain intact.
* `tools/props/parts/office_refined.py`, `tools/props/build_office_textures.py`,
  small routing changes in `parts/furniture.py` and `parts/appliances.py`.
* `tools/textures/office_art.py`, Office-only changes in `lights_art.py` and
  the stained-wallpaper preset in `seam_repair.py`.
* `tests/test_office_assets.py`, `tests/fixtures/levels/office_asset_review.json`,
  `tools/bench/capture_office.py`.
* One density-expectation update in `src/static_prop_lighting_tests.rs` needed
  for the rebuilt Hollow package; actual metadata-cap assertions are retained.
* `docs/ASSET_SPECIFICATION.md`, this report.
* Four dependent `assets/levels/*.placesmap` outputs listed above.
