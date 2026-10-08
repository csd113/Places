# 2026-10-08 — Stage 1: visual gap audit and hero baseline

This milestone establishes a benchmark. **It does not claim a renderer or lighting
improvement.** The normal engine now has a compact, repeatable room/hall/garden
fixture, six native High views, two Low controls, and three independently repeated
High views with byte-identical PNGs. The completed asset passes remain intact.

[Chronological visual journal](../style-upgrade-20261007/README.md) ·
[Dependency plan and visual criteria](gap-plan.md) ·
[Costs and measurement limits](performance.md) ·
[Exact deferred ledger](deferred-ledger.json) · [Validation](validation.md) ·
[Development diagnostics and expanded evidence](diagnostics.md) ·
[2026-10-08 Stage 2 colour/material milestone](stage2/README.md) ·
[2026-10-08 Stage 5 presentation/environment milestone](stage5/README.md)

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
interior. No new texture or material class was introduced. The opt-in diagnostic selector
inspects existing routes; the normal release composition remains unchanged.

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

## Expanded diagnostic foundation

The strengthened Stage 1 contract adds an opt-in `visual-diagnostics` build with
11 selectable existing-data modes, F8 cycling, capture-time resident-state
receipts and actual offscreen encoding counts. It adds saved-package lighting
export and scoped live solver provenance, while preserving the physical bake,
package format and normal release. [Three normal after-build captures](diagnostics/normal-preservation-comparison.json)
and the feature's final entity view match the original baseline byte for byte.
The single dump-enabled forced hero bake produces the **same package bytes**.

[Twenty-five cases / 37 raw native diagnostic and control images](diagnostics/campaign-execution.json)
cover all modes, ordinary Low/Medium/High, independent atlas/filtering, the Low
lighting override and settled live transitions. All receipts agree on requested,
applied and resident settings, uploaded mode and the one spawned chair.
[Six control pairs](diagnostics/control-comparisons.json) are byte-identical,
including returned High/Full/filtering. The ready-only gate cannot prove absence
of transient blank/stale frames during graphics loading; that limit stays in
[the ledger](deferred-ledger.json).

The new evidence locates the sofa's diagonal shading in combined atlas mean and
physical direct transport, rather than its PNG or post alone. True offline charts
show separate triangle grids on coplanar cushions. The additional resin-table
view is severely underlit despite light-colored artwork; it retains faint detail
and is not a literally all-black model. Its combined baked-light view is also
dark. At its authored X/Z anchor, the hall panel's horizontal footprint distance
is 5.2 m, beyond its 5 m falloff range. The other distance-eligible room panel's
centre path crosses the partition; actual tap visibility is not measured here.
This qualifies direct source coverage at one point, not total indirect/sky support.
Stage 3 must separate authored energy from processing loss; no asset replacement
is justified. Finite panel shapes and sampled soft shadows already work.
No nonemissive full-white model or new physical opening leak is demonstrated.

[Diagnostic contracts, native galleries and labeled offline projections](diagnostics.md)
explain unavailable data honestly. Direct/indirect are real offline components;
chart-UV is only atlas addressing; shadow queries identify actual triangles;
no standalone AO, metallic shader input or runtime probe overlay is invented.
Stage 4 has an explicit extension seam for a real probe visualization provider.

## Reference and pipeline audit

Three independent reviewers used GPT-6.1-sol with `xhigh` reasoning for the
rendering/color, baker/geometry, and concepts/assets/constraints audits.
The integrator checked the claims against current source, manifests and the new
native views. A preliminary reviewer statement that the demo lacked a sky was
corrected: its actual sky is present with zero ambient. The original audit was read-only. After the strengthened Stage 1 request, the
runtime specialist owned the feature selector/native receipts and the compiler
specialist owned diagnostic provenance/export code in disjoint files. The lead
integrated, built, tested, captured and remained the sole report/Git writer; the
visual reviewer stayed read-only. The lead reconciled expanded native/offline
evidence against source; the visual reviewer qualified the dark table and the compiler reviewer qualified
the six caster queries.

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
in 2,029 passing library tests or the remaining scoped integration checks.
The initial background timing attempt had zero scene draws and remains invalid
for scene performance. A later bounded normal-build campaign submitted every
measured frame: High room median CPU/event loop 7.676 ms, 31 accounted base-scene
draws, with 433.3 MiB process RSS. This is a local submission baseline, not GPU
time or a cross-platform frame budget. No quality/performance improvement is claimed.

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

## Later milestones

- [2026-10-08 — Stage 3: light across construction seams](stage3/README.md)
- [2026-10-08 — Stage 4: light that follows objects](stage4/README.md)

Each stage retains its original raw captures, settings and runnable snapshot
receipts. The Stage 4 handoff supplies publication identities after its seal.
