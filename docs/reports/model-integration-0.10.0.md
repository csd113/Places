# Completed model integration — 0.10.0 staging

Adopted all 107 finished identities: 83 original Arcade models, two new benches,
16 Shared props and six entity rigs. Their 103 canonical GLBs remain byte-identical
to the verified v36 delivery. The five streamer colours share one neutral mesh
and bounded linear albedo tints; decoded family images and GPU uploads share
content identities. All 78 required unique PNG companions exist under assets.
The full accepted inventory, hashes and bounds are in assets/completed-models.json.

The three original archives, 191 editable payloads, checklist, manifests and
adoption notes remain on this Mac in Downloads/Places-completed-models-2026-10-14.
The included verify_delivery.py checked 372 files, 107 identities and 103 GLBs,
with no missing files, mismatches or unexpected runtime models. The delivery
manifest identifies master v36 with SHA-256
f5b832903a64c5c8ffea786ff2f568f89f6917b7077f39a2a0b2f500b87188d6;
the original master JSON itself was not bundled for an independent rehash.
Complete transfer results remain private in local/model-integration-0.10.0.

Catalog envelopes follow actual pivots and measured bounds. Cabinet positions
retain their original layout; light offsets follow the accepted exports. Pinball,
chair, cat and restroom envelopes were fitted. Six current cosmetic ball heights
follow the final triangle-contact manifest; the newer playable cobalt ball stays
on the approach. The reset switch now fits the return's accessible rear face.
Both opposite-upholstery benches occupy the east Arcade waiting wall. Native
movement stops against a seat and passes through their gap. The cat remains a
standalone prop and is absent from the Bepsi vending placement.

The pin's seven convex slabs follow its delivered profile; ball/pin masses remain
5 kg/1.5 kg with derived inertia. Gutter inner support and existing hulls remain
unchanged, including the documented 4.502298 mm inherited over-support and at most
1.154944 mm exterior refinement. Water buoyancy, drag, upright player dimensions,
dormant starts, carrying and bounded recovery remain intact.

Cake candle_flicker, dim candle glow, named balloon body/string nodes and anchored
analytic idle motion are preserved. Cloth/gift fluorescence uses the actual UV
masks; metal housings remain nonemissive. Four hash-verified rectangular cloths
retain their 1.0495164 U overscan with clamp sampling. Ordinary UV limits and the
106 MiB unique decoded pack budget remain: 75.23 MiB shared versus 194.13 MiB
without image sharing. Historical exporters protect completed hashes rather than
regenerating the superseded models.

Denser geometry requires eight Full atlas pages. The typed record allowance is
384 MiB plus 64 KiB, retaining exact shape checks, the 512 MiB KTX2 limit and
1 GiB aggregate cap. A focused grazing-moment regression exposed unstable f32
calibration; finite surface energy now uses an isotropic representation when that
encoding fails the unchanged strict energy check. Arcade's successful 26m15s bake
contains eight pages and 1,575 irradiance probes, with no Full fallback. All eight
supported packages passed require-current verification. Archived places_demo and
model_zoo remain unbaked JSON history; default Rust checks and CI bake no maps.

| Actual validation | Result |
|---|---|
| cargo fmt --all --check | Passed |
| cargo clippy --workspace --all-targets --all-features -- -D warnings; release equivalent | Passed |
| cargo test --workspace; --all-features | 2,320 / 2,326 passed; 19 existing GPU-only ignores |
| Source Python suite; asset audit; generator/clip checks | 355 tests, four skips; zero audit errors; passed |
| Native bowling / published normal-transform campaign | 944 / 468 checks passed |
| Native water acceptance | Six scenarios, 243 checks passed |
| Native scenes / rigs / materials | Eight supported scenes, 19-model fixture, all 24 rig clips; passed |

Native High/Full captures verify both bench colours, Bepsi, cake/balloon motion,
UV off/on/restored, mirrors and opening doors. Fixed cabinet far/near/far images
are RGB-identical; static UV regions restore exactly. Static mode is confirmed
by the renderer's off-atlas runtime state. Ten curated original 1280×720 captures
are under local/model-integration-0.10.0/screenshots. Zero models failed or assets
were missing; four untextured draws intentionally use the committed white PNG.
Moving restroom leaves retain the existing parametric implementation; the remade
static stall-door GLB was verified in the explicit fixture.

Two alternating, idle 600-frame pairs with 120 warmup frames use the exact same
clear eye (6.7,1.6,9.5), yaw/pitch (275,0), Apple M2 Pro/Metal, 1280×720 drawable,
High textures/filtering, Full lighting/reflections and bloom. The baseline uses
the verified water-stage binary/package and exact original assets, including
four runtime resources outside its package dependency list. Preliminary incomplete
or collision-shifted comparisons are excluded.

| Matched cabinet view | Water baseline | Adopted models |
|---|---:|---:|
| CPU frame median | 1.660–1.661 ms | 1.978–1.993 ms |
| CPU frame p95 | 3.183–3.238 ms | 2.475–2.787 ms |
| GPU median | 1.203–1.331 ms | 1.373–1.390 ms |
| Visible triangles / draws | 24,804 / 123 | 77,774 / 99 |
| Animated vertex slots | 12,256 | 24,516 |
| Peak native RSS | 889–958 MiB | 1,379–1,406 MiB |

The mean CPU-frame increase is 0.325 ms (19.6%). More approved animation vertices
and 3.14× visible geometry explain the added work; render submission stays stable
and image hashes run during uploads, outside the frame loop. No avoidable repeated
hashing/upload overhead was found. Atlas CPU/GPU storage grows by 288/144 MiB,
explaining most of the roughly 470 MiB RSS increase. At this view, CPU/GPU times
remain below a nominal 16.67 ms frame budget; this is not a worst-case or 1080p
claim. Captures, body traces and concurrent-bake diagnostics are excluded from
these timings. Exact receipts, branch/remote/CI evidence, runtime preservation
and final ownership release are recorded in the local handoff.
