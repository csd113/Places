# Fixture housing and emission audit — 0.11.0

The two Office shaft sconces do **not** have missing backplates. Their delivered
`core:pool_light_wall` model has a closed metal rear, correct normals and ordinary
non-emissive metal/gasket materials. Native front, side and wide views in the
real Office map show the closed body. Broad illumination in front of a sconce
and light received by metal in Static are distinct from escaping through its back.
No delivered Office or Pool fixture GLB, wall placement, source power or colour
was changed. Baseline: audio commit `55072c92f9558b0febe189a2217a9a5fec72bc90`.

The audit did find related defects in the procedural fixture path and two Home
models. Those are corrected in their implementation/authoring sources, rather
than through global lighting adjustments.

| Corrected scope | Cause and repair |
| --- | --- |
| Procedural Home, blacklight and Pool model fallbacks | Bare bodies shared the `Light` draw family with luminous faces and were excluded from Full transport. Bodies now cast opaque shadows and receive ordinary baked lighting; their authored paint shades remain albedo. Fitted luminous endpoint sheets remain separate. |
| Home flush mount | Open mounting back; source at the mount, 70 mm above the diffuser; square source samples reached the rim. Close the ten-sided back and use the actual diffuser plane with an inscribed aperture. Move the center boss behind that plane, matching its documented design and avoiding loss of the central Full source sample. |
| Blacklight batten | Already closed, but its body did not occlude and its source sat 46 mm above the lens. Retain the body and fitted 8:1 PNG; align source extent and plane to the 1.12 × 0.14 m lens. |
| Pool round/wall procedural fallbacks | Add the missing mounting backs. Recess the round fallback face to the shared 17 mm source plane so its new cap does not coincide with the diffuser. Delivered GLBs remain unchanged. |
| `home:floor_lamp` | The first registered material was luminous and also owned the already-built base/stem. Register ordinary `lamp_body` before those 80 triangles; preserve the 144 shade triangles and their emission. |
| `home:cabinet_strip` | The same registration error made its housing luminous. Register ordinary `strip_housing`; replace two exposed underside triangles with eight aperture-border triangles around the unchanged lens, removing a 0.02754 m² coplanar overlap. Final model: 18 ordinary + 12 luminous triangles. |

The earlier fluorescent troffer repair is retained: its opaque backplate,
brushed-metal material, recessed fitted diffuser, UV orientation and source
contract are unchanged. Geometry revision 16 invalidates stale prepared meshes
and lighting for the procedural changes. No solver tolerance, atlas density,
ambient setting, exposure, shadow policy or authored source intensity was reduced.
No PNG was created, resized or repainted.

The complete material scan covered 366 distinct GLBs, including 42 luminous
models. A separate topology/normal/role audit covered 21 lamp and fixture GLBs:
Office and both Pool housings; core lamp/exit sign; Home ball/floor/strip lights;
Outdoors wall/fence/stand/street lights; Aero pod/ring/soffit; Arcade blacklight
and neon; all three Winter string spans and the timber lantern. All six built-in
fixture IDs and their procedural implementations were inspected. No additional
missing mounting backs or bad authored normals were found. Touching assembly
seams and intentionally buried faces were distinguished from exposed overlap.

Office's `shaft_lower` and `shaft_middle` remain at X=35.86, Y=2.1/5.7,
Z=2/16, yaw 270°, with 20 mm rear wall clearance. Independent triangle queries
using Full's actual 3×3 stencil (18 taps across two strips) blocked all **1,386**
sampled mounting-wall rays on ordinary metal and left all **450** sampled front
rays clear. Source clearance exceeds the actual endpoint tolerance. This rejects
the missing-backplate hypothesis for those placements; it does not establish
exhaustive visibility at every possible receiver or arbitrary yaw.

Independent review checked the shared routing, source planes, outward cap
winding, nondegenerate geometry, quad budgets and unchanged delivered fixtures.
It also independently decoded both Home exports. Floor-lamp triangle attributes,
ordering, UVs, colours and PNG bytes are unchanged. The strip's other five housing
faces and complete lens remain unchanged; its new perimeter preserves the original
affine underside UV mapping (maximum numeric error 2.81×10⁻⁸), closes the exterior
without T-junctions and has zero exposed housing/lens overlap. Scratch regeneration
is deterministic and matches both shipped outputs.

Native acceptance uses the direct executable through SDL3/wgpu Metal at a real
1600 × 900 drawable. Isolated compact controls cover all six fixture IDs, lit
and switched-off states, front/side/detached rear views, and a moving flashlight.
Separate asset roots exercise the model fallbacks. Production transport queries
now block six Home/blacklight ceiling-side witnesses on actual housing triangles
while leaving both central forward-source rays clear; all eight witnesses were
unblocked before. Sampled lamp-shade and strip-lens image regions match exactly
before/after, independently of their now-dark ordinary bodies.

| Genuine native evidence | Before → after |
| --- | --- |
| Home drum side / closed rear | [side before](../images/fixture-leaks/home-side-before.png) → [side after](../images/fixture-leaks/home-side-after.png); [rear before](../images/fixture-leaks/home-back-before.png) → [rear after](../images/fixture-leaks/home-back-after.png) |
| Floor lamp, unchanged shade | [before](../images/fixture-leaks/lamp-before.png) → [after](../images/fixture-leaks/lamp-after.png) |
| Cabinet strip, unchanged lens | [before](../images/fixture-leaks/strip-before.png) → [after](../images/fixture-leaks/strip-after.png) |
| Actual Office sconce, unchanged closed metal body | [side audit](../images/fixture-leaks/office-side.png) |

Passed production checks: `sh tools/check-rust.sh` (format, locked check, strict
debug/release all-target/all-feature Clippy, 26 existing CPU tests and locked
all-feature release build); asset validation, texture and prop checks; Office
and Arcade generator currency; 372 Python source tests (four explicit native
skips); patch whitespace. No new Rust tests were added. Texture checks retain
96 existing manifest advisories; there are no new validation failures.

The explicit installed-map refresh scope is Home, Arcade, Office, Winter, Aero
and Lantern Hollow: the Home fixture is reused by the latter three, while Office
is the unchanged model control. Each build is followed by package validation and
`verify --require-current`; completed outcomes and exact hashes are retained in
the local `package-builds.json` receipt. Pool and Beach have no affected active
fixture path and are not refreshed. Compiled packages remain untracked.

Static retains its existing coarse visibility model without fixture self-occlusion;
its ordinary body receiver colours are now lit, but this pass does not turn Static
into Full transport. Reflective captures retain their authored initial state.
The geometry audit also recorded pre-existing enclosed Outdoors light anchors as
a separate self-occlusion limitation; they are not a missing-back/side-leak defect
and were left unchanged. No Linux/Windows native or FPS claim is made.

Raw source/asset proofs, exact camera and switch recipes, original/current
executables, package hashes, logs and genuine captures are preserved outside
`target` in `local/fixture-leaks-0.11.0/`. No archived map was baked. No matching
open issue was found, so no issue was closed. Caustics remains a separate stage.
