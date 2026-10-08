# 2026-10-08 — Stage 1: visual gap audit and hero baseline

This milestone establishes a benchmark. **It does not claim a renderer or lighting
improvement.** The normal engine now has a compact, repeatable room/hall/garden
fixture, six native High views, two Low controls, and three independently repeated
High views with byte-identical PNGs. The completed asset passes remain intact.

[Chronological visual journal](../style-upgrade-20261007/README.md) ·
[Dependency plan and visual criteria](gap-plan.md) ·
[Costs and measurement limits](performance.md) ·
[Exact deferred ledger](deferred-ledger.json) · [Validation](validation.md)

## Matched native evidence

These are raw PNGs written by the SDL3/wgpu/Metal player's normal capture path.
There is no retouching, compositing, image generation, or camera-specific shading.
The right column is a separate launch of the same baseline, rather than an
improved after state. [Numeric comparison](repeatability.json) confirms all
921,600 pixels match in each of these three pairs.

| Baseline | Independent repeat |
| --- | --- |
| ![Room baseline](baseline/high/room.png) | ![Room repeat](repeat/high/room.png) |
| ![Window baseline](baseline/high/window.png) | ![Window repeat](repeat/high/window.png) |
| ![Static and spawned chairs baseline](baseline/high/entities.png) | ![Static and spawned chairs repeat](repeat/high/entities.png) |

Additional fixed views: [hall and materials](baseline/high/hall.png),
[contact grounding](baseline/high/contact.png), [corner and passage](baseline/high/corner.png).
The normal Low controls are [window](baseline/low/window.png) and
[entities](baseline/low/entities.png); their effective Low/Off settings are
confirmed in the corresponding native logs. Low is a diagnostic quality reduction,
not the visual target or an alternative art library.

## Reproduce and play

Run from the repository root. The package is a reproducible local artifact outside
`target/`; source, settings, cameras and requested evidence are tracked.

```sh
RUSTC_WRAPPER= cargo build --release
mkdir -p debug-maps/art-style-hero/evidence
target/release/places-compile build tests/fixtures/levels/art_style_hero.json \
  --out debug-maps/art-style-hero/evidence/art_style_hero.placesmap --workers 12
python3 tools/bench/capture_art_style_hero.py --play
python3 tools/bench/capture_art_style_hero.py --out /tmp/hero-next-high
python3 tools/bench/capture_art_style_hero.py --out /tmp/hero-next-low \
  --quality low --views window,entities
```

`RUSTC_WRAPPER=` disables this host's unavailable sccache process; it is not a
project setting change. Native compiler/capture commands need a working Metal
desktop adapter. In the selected environment they required reviewed escalation.
Output directories must be empty; the capture tool refuses to overwrite evidence.
It isolates settings and level discovery in temporary roots, verifies the requested
world, effective graphics settings and drawable dimensions, and records source,
catalog, package, executable and camera identities. Later revisions of the tool
also include its own hash and the exact effective-settings receipt.

[Camera and bake manifest](hero-manifest.json) fixes the camera position/yaw/pitch,
60° FOV, 640×360 logical/1280×720 drawable resolution, quality controls, bloom,
environment and capture at 0.5 ready-world seconds. Spawn arrays are `x,z,yaw`;
the normal player eye is 1.6 m above the floor. The chair spawns at 0.1 s and stays
still, so no animation/AI/weather timing affects the comparison. The native
[baseline](baseline/high/manifest.json), [repeat](repeat/high/manifest.json) and
[Low](baseline/low/manifest.json) receipts preserve exact build/package identities.

The fixture is an 8.15×6 m furnished room with a 2.35 m wide connected hallway
and a sealed 3.2×1.25 m window into a real open garden. It reuses the finished
Home sofa, table, CRT, books, mug and lamp; Pool resin table; and Outdoors tree,
shrub, fence and lantern. Carpet, oak, tile, warm paint, plastic, brushed metal
and blended clear glass use existing catalog definitions. Two ordinary broad
fluorescent fixtures, a warm practical point source and garden light use normal
authoring paths. The garden has the existing star sky, ambient 0.22 and a baked
cool directional source; it is decorative and inaccessible from the playable
interior. No new texture, material class or runtime renderer path was introduced.

## What the current evidence establishes

The warm practical creates a visible wall/floor/ceiling pool, the broad panel
illuminates the room, static furniture casts real contact shadows, the window
has actual reveals and transmitting glazing, and the hallway connects without
an obvious hole. The [corner view](baseline/high/corner.png) remains coherent
across its wall junction. These working behaviors are acceptance anchors.

The [contact view](baseline/high/contact.png) shows darker, diagonal model-face
gradients and abrupt contact regions compared with the soft cream upholstery
and restrained timber in the Home concepts. The [entity pair](baseline/high/entities.png)
shows a richly shaded static chair on the left and a flat, ungrounded spawned
chair on the right. Both use the same existing GLB, scale and orientation.
Their 0.9 m position difference makes this a qualitative consistency check,
not an equal-position radiometric test. The [probe diagnostic](probe-report.json)
finds valid, nonzero nearby probes, no adjacent same-room 100× candidates, and
similar upward probe luminance at the two requested locations (about 0.612 and
0.603). Nearest probe/floor samples are approximate and are not chair surface
measurements. The visible difference cannot simply be called a missing field.

The [window](baseline/high/window.png) retains very dark exterior forms even
with supported authored sky/moon energy. Its Low counterpart reads the tree and
fence more clearly. This confirms different responses in these two paths, not
the absence of a sky-light feature. Places Demo has sky ambient **0.0** and no
global illuminator, while Lantern Hollow and Winter author both; missing energy
in the demo is a separate content/default issue. Equal-source contribution
exports must distinguish it from transport or presentation loss before a fix.

The [hall](baseline/high/hall.png) makes plastic and brushed metal available at
one camera, but their response remains subdued. Architectural normals/sheen
and probe reflection already work; prop materials have fewer supported controls.
The opening's dark reveal/top edge is consistent with shading and geometry in
this baseline. No new crack, atlas corruption or physical opening leak is proven
by this six-view campaign. Historical prop seams and thin-shadow limits require
the focused controls in the plan rather than a universal brightness patch.

## Reference and pipeline audit

Three independent reviewers used GPT-6.1-sol with `xhigh` reasoning and read-only
ownership: rendering/color, baker/geometry, and concepts/assets/constraints.
The integrator checked the claims against current source, manifests and the new
native views. A preliminary reviewer statement that the demo lacked a sky was
corrected: its actual sky is present with zero ambient. No reviewer wrote files
or ran builds, tests, bakes or captures.

All seven immutable concept PNGs across Office, Pool, Home, Outdoors, Winter
and the entity atlas were visually read; [reference inventory](reference-inventory.json)
records paths, dimensions and SHA-256 identities. Existing asset-pass native
galleries were inspected, including High and Low Outdoors and severe Winter.
Pool's blue basin/mineral deck/resin construction, Home's domestic furniture and
Shaker stock, Outdoors' fitted foliage/lanterns and Winter's masonry/timber/snow
are genuine gains. Their remaining scene-scale, density and silhouette gaps do
not justify discarding the completed assets. Some Office/Halloween evidence is
historical; it is not presented as a fresh head capture.

| Subsystem | Current contract and consequential limitation |
| --- | --- |
| Color and targets | PNG channels/mips/shading operate in display space; final sRGB conversion preserves displayed bytes. This is deliberate GLES compatibility, not an accidental double-gamma bug. Scene/emission/bloom/reflection targets are RGBA8, and prepared HDR light is soft-clipped before albedo multiplication. See `materials/image.rs`, `render/wgpu/texture.rs`, `world.wgsl::surface_light` and `postprocess.rs`. |
| Compiler and formats | Explicit v3 source → validated content-addressed package → native runtime. Off/Medium/Full variants, collision/navigation, reflection captures and an irradiance field are already prepared offline. Current solver revision 11, geometry revision 3 and lightmap cache version 13 participate in reuse; any changed deterministic result must invalidate the relevant fingerprints. |
| Baker and charts | Triangle visibility, HDR diffuse transport, directional moments, endpoint-centre UVs, gutters, geometry-side origins and supported architecture filtering already exist. Prop charts remain separate triangles; `SurfaceFilter::across_edge` excludes them. Inclusive production chart grids can differ after segmentation; analytical controls use manually matched grids. |
| Static shadows | Real opaque model triangles cast prepared shadows independently of the fast `occludes` flag. Fallback uses coarse prop boxes. Cutout model cards transmit as whole triangles, analytic fixture housing is omitted, and moving leaves/entities do not cast static transport shadows. Finite visibility samples can miss a narrow caster entirely. |
| Geometry and openings | Shared wall solid-slice rules govern mesh/collision/fast occlusion; authored floors must meet at thresholds. Height-aware wall ownership and gable clipping work. Geometry checker resolution cannot prove the absence of tiny illumination leaks. A dark jamb is not proof of an uncut hole. |
| Materials | World surfaces support tint, normal maps, sheen, emission, alpha and selective reflection. GLB resolution currently imports base color/emission/alpha, not normal/roughness/metallic response. Static BLEND props become opaque; character MASK becomes opaque. Translucent sorting and emissive coverage also differ by route. |
| Entities | Valid air-labelled, visibility-supported probes exist. Runtime samples a model-bounds centre and reconstructs a per-object response. Architecture/probes receive recovery fill, while props deliberately do not. This policy, sampling and absent dynamic grounding must be diagnosed separately. |
| Reflections | Existing probes and one selected planar surface work. Main sky is drawn in the scene, but sky is omitted from probe/planar captures. Camera-selected probe switching and lack of spatial correction are separate limitations. |
| Post and atmosphere | Bloom, fixed shoulder/exposure/grade, normal fog and shelter-aware Winter storms exist. Ordinary fog differs across world, decal, effect and emission paths; regional ordinary fog uses fragment-local density. No edge AA path is implemented. Winter storm sightline integration must remain intact. |
| Assets and platform | Real committed PNGs, fitted 256² native prop atlases, catalog validation, topology/UV/source checks and quality-scaled upload caches work. This host is M2 Pro/Metal; Linux hardware and Windows native performance are not measured here. High is the fresh-install visual target. |

The [gap plan](gap-plan.md) includes source locations, confidence, visible benefit,
cost/risk and reproducible checks for each major issue. It recommends a dependency
allocation to Stages 2–7 without selecting a lighting or rendering algorithm.

## Acceptance and custody

Build, all-variant hero bake, package integrity/currency, six native High views,
matched repeatability, controls, audit, costs and the migration ledger are
established. The local whole-workspace test command is **not green** because of
the explicitly inherited packages in the ledger; no source regression appeared
in 2,022 passing library tests or the remaining scoped integration checks.
Reliable live scene FPS/GPU timing is also unavailable in this background window
session; resource and bake costs remain valid. Neither limitation is concealed
as an achieved quality or performance improvement.

The selected task exposes workspace-write sandboxing and reviewed per-command
escalation, not a verifiable persistent Full Access switch. Its global config
names GPT-6.1-sol but records medium reasoning; active lead reasoning cannot be
queried/changed through the exposed task controls. Reviewer model/`xhigh` settings
were explicitly configured. No callable `/goal` API or installed goal skill was
exposed after tool/local-skill inspection; this work used the supported delegated
task workflow. These execution-control limitations were reported before authoring.
The coordinator must verify the requested controls when launching later stages.

Implementation and final receipt links are recorded in [handoff](handoff.md).
Stage 2 was not started. The stage's writer/index/target ownership and healthy
inherited `caffeinate -di` PID 88945 return to the coordinating parent at handoff.
The untracked custody record is `/tmp/places-art-style-queue-custody.json`.
Build artifacts stay available for the unfinished serial queue; no `cargo clean`.
