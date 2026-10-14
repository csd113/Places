# Completed model integration — 0.10.0 staging

Adopted all 107 finished identities: 83 original Arcade models, two new benches,
16 Shared props and six entity rigs. Of 103 canonical GLBs, 102 remain byte-identical
to v36; Bepsi has the narrow print trim documented below. The five streamer colours share one neutral mesh
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

Close native inspection exposed two delivery/integration defects. Bepsi's lower
printed opening overlapped its physical rim by 15–17 mm at the same depth. The
hash-guarded tools/props/trim_bepsi_dispenser.py moves 12 print vertices and their
affine UVs to leave a 0.2 mm gap. Exactly 96 bytes change; geometry elsewhere,
indices, normals, materials and images are unchanged. The original v36 archive
and editable master remain intact in Downloads.

Sink and toilet textures, normals and delivered meshes were correct. Full
lighting receivers could adopt a neighboring shallow-fold face, then clip their
sampling stencil against that different plane. Solver revision 21 uses the
existing numerical coplanarity tolerance for receiver ownership. The two-face
regression now covers both hard and shallow folds without tracing rays or baking
a map. In the diagnostic sink seam, the direct-light discontinuity falls from
0.3378 to 0.00108. Native fixture captures remove the broad wedges/splits while
retaining actual cavity, faucet and waterline shadows. No porcelain emission,
texture repainting or gameplay brightness compensation was introduced.

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
encoding fails the unchanged strict energy check. The final solver-21 Arcade
30m05s bake contains eight pages and 1,575 irradiance probes, with no Full
fallback and no compiler warnings. A sandboxed attempt could not access Metal
for final reflection capture; the successful run used verified native GPU access.
All eight supported packages passed require-current verification and fresh native
scene checks. Current receipts are preserved locally. Archived places_demo and
model_zoo remain unbaked JSON history; default Rust checks and CI bake no maps.

| Actual validation | Result |
|---|---|
| cargo fmt --all --check | Passed |
| cargo clippy --workspace --all-targets --all-features -- -D warnings; release equivalent | Passed |
| cargo test --workspace; --all-features | 2,320 / 2,326 passed; 19 existing GPU-only ignores |
| Source Python suite; asset audit; generator/clip checks | 356 tests, four skips; zero audit errors; passed |
| Native bowling / published normal-transform campaign | 944 / 468 checks passed |
| Native water acceptance | Six scenarios, 243 checks passed |
| Native scenes / rigs / materials | Eight supported scenes, 19-model fixture, all 24 rig clips; passed |

Native High/Full captures verify both bench colours, cake/balloon motion,
UV off/on/restored, mirrors and opening doors. Fixed cabinet far/near/far images
are RGB-identical; static UV regions restore exactly. Static mode is confirmed
by the renderer's off-atlas runtime state. Thirteen curated 1280×720 captures
are under local/model-integration-0.10.0/screenshots. Zero models failed or assets
were missing; four untextured draws intentionally use the committed white PNG.
Moving restroom leaves retain the existing parametric implementation; the remade
static stall-door GLB was verified in the explicit fixture. Full and Static
Arcade follow-up runs each passed eleven camera captures with zero unpresented
scene events, including four Bepsi angles and sink/toilet interiors. Three
defect captures are also saved to Library, with complete transfer results and
version xattrs retained.

Two final alternating idle 600-frame pairs with 120 warmup frames use the same
clear eye (6.7,1.6,9.5), yaw/pitch (275,0), Apple M2 Pro/Metal, 1280×720 drawable,
High textures/filtering, Full lighting/reflections, bloom and vsync off. The
baseline uses the verified water-stage binary/package and original assets,
including four runtime resources outside its package dependency list. Every
run presented all 720 ready frames with no misses. No owned compiler, captures,
body traces or scripted motion were active. Earlier valid adoption measurements
remain separately preserved; incomplete/collision-shifted preliminary runs are
excluded.

| Matched cabinet view | Water baseline | Final adopted models |
|---|---:|---:|
| CPU frame median | 2.594–2.670 ms | 2.624–3.133 ms |
| CPU frame p95 | 6.652–6.779 ms | 6.364–6.426 ms |
| Update median | 1.181–1.200 ms | 1.531–1.534 ms |
| GPU median | 2.422–2.480 ms | 2.012–2.998 ms |
| Visible triangles / draws | 24,804 / 123 | 77,774 / 99 |
| Animated vertex slots | 12,256 | 24,516 |
| Peak native RSS | 834–847 MiB | 1,246–1,272 MiB |

The mean CPU-frame increase across these pairs is 0.246 ms (9.4%); update cost
rises about 0.342 ms. GPU and submission timings vary between pairs, so neither
pair establishes a stable gameplay FPS result. Approved animation vertices and
3.14× visible geometry add work, while draws fall. Image hashes run during
uploads, outside the frame loop. Atlas CPU/GPU storage grows by 288/144 MiB,
accounting for most of the roughly 418 MiB RSS increase. This is one idle view
at 1280×720, not the user's actual-resolution 60-versus-120 FPS case or a
worst-case campaign. Multicore implementation and that independent Arcade
investigation remain the next stage; they were not started here. Exact receipts,
branch/remote/CI evidence, runtime preservation and ownership release are
recorded in the local handoff.
