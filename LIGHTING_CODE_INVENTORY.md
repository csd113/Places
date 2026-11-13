# Places lighting code and dependency inventory

Inspected **2026-11-13**, on `0.14.0-dev` at `085b0deb` (the game/rendering source
is unchanged from `48ea8268`). This is a planning inventory, not authorization to
delete or rewrite anything. No lighting implementation or level was changed to
produce it. Paths are relative to the repository root; directory groups include
their children unless a narrower list is given.

The lighting system is not contained in `src/engine/lighting/`. Its offline
solver, geometry preparation, runtime receiver evaluation, GPU shaders, content
formats and editor controls are separate owners. Deleting the lighting directory
alone would leave shader logic, saved-format contracts, runtime shadows and
lighting-specific geometry/material code behind, while breaking their imports.

**Labels:** **Core** calculates or applies illumination/visibility; **Bridge**
passes, prepares or stores lighting data; **Authoring** exposes or validates it;
**Check** tests/diagnoses it; **Data** is authored or generated content. A bridge
or consumer is a dependency to preserve/adapt, not automatically a deletion target.

## 1. End-to-end ownership and starting points

```text
Editor + level/project JSON + catalog/GLB/PNG materials
    -> loader/validation + LevelDef + material/prop resolution
    -> renderer-neutral geometry and source/visibility preparation
    -> offline light transport, atlas, irradiance field, reflection captures
    -> versioned .placesmap records and preparation/dependency identities
    -> package/world loader -> staged renderer installation
    -> live fixture/circuit state, moving receiver probes and visibility
    -> WGSL material/shadow/UV/caustics/reflection/fog evaluation
    -> HDR scene/emission -> bloom -> exposure/tone/grade -> display transfer
```

| Starting point | Role and connections |
| --- | --- |
| `src/map_compiler/mod.rs` | **Bridge/Core orchestration.** Offline Static/Full preparation, geometry, lighting, probes, package records and stage/cache decisions. |
| `src/engine/render/common/api.rs` | **Core/Bridge.** Neutral `LevelBuild`, geometry preparation, lightmap fill requests, transport/probe solve handoff, preview and vertex-lit paths. Crucial boundary between editor preview and final preparation. |
| `src/engine/package/world.rs` | **Bridge.** Loads prepared lighting, atlases, irradiance, reflection captures and geometry; checks required Full lighting and cross-record references. |
| `src/engine/render/wgpu/renderer.rs` | **Core orchestration.** Installs prepared resources, updates live light/receiver state, orders scene/shadow/reflection/post passes and changes graphics settings. |
| `src/engine/render/wgpu/world.rs` and `world.wgsl` | **Core.** Pipeline/vertex/binding contract and shared material lighting for architecture, props and characters. WGSL is assembled with other shader fragments, not a self-contained file. |
| `src/game/app/loading.rs`, `src/game/app/mod.rs` | **Bridge.** Prepared world installation and runtime renderer ownership. The normal player consumes compiled lighting; it must not silently bake a shipped map at startup. |
| `src/editor/viewport/preview.rs`, `src/editor/bakes.rs` | **Authoring/Bridge.** Approximate editable previews versus explicit final build/cache management. These are different paths and need separate acceptance. |

Some older module comments still say “baked once per level load.” Follow the
current compiler/package call chain above when deciding what runs in gameplay.
Likewise, older reports' format/revision numbers can differ from current source.

## 2. Authored rules, asset resolution and validation

| File/group | Role and preservation boundary |
| --- | --- |
| `src/engine/level.rs` | **Authoring/Data contract.** `LightFixtureDef`, `LightDef`, `LightShapeKind`, mounting, switch modes, fixture emission, prop lights/emission overrides, circuits, `shadow_only`, `LightResponseDef`, glow, water/caustics, sky/fog and `requires_full_lighting`. Also contains shared architecture and action definitions; do not delete the file as a lighting unit. |
| `src/engine/level/celestial.rs` | **Core/Authoring.** Sun/moon direction and visible celestial disc data; light direction must agree with rendering. |
| `src/engine/level/construction.rs`, `src/engine/spatial.rs` | **Bridge.** Actual surface geometry, fixture placement, mesh attributes and spatial batching. Contact, openings, normals and lightmap boundaries depend on them. |
| `src/engine/assets.rs`, `src/engine/loader.rs` | **Authoring/Bridge.** Catalog schemas/resolution, fixture-family sheets, source materials/models and prepared level inputs. |
| `src/engine/loader/validation/lighting.rs` | **Authoring.** Fixture/prop source limits, finite values, blacklight and caustics compatibility, controllable-light constraints. |
| `src/engine/loader/validation/mod.rs`, `props.rs`, `geometry.rs`, `motion.rs` | **Authoring dependencies.** Dispatch, shadow-only placement restrictions, openings/storeys and moving/attached light constraints. |
| `src/engine/materials.rs`, `src/engine/materials/resolve.rs`, `pack.rs` | **Bridge/Data contract.** Catalog and map-owned materials become resolved draw/bake inputs; IDs and authored overrides are saved-map dependencies. |
| `src/engine/materials/emission.rs`, `response.rs`, `reflection.rs`, `fluorescence.rs` | **Core policy.** Surface emission, normal/specular/roughness/alpha, reflection routing and UV pigment response. Emission makes a surface bright; it is not automatically a light source illuminating other objects. |
| `src/engine/materials/color.rs`, `image.rs`, `builtin.rs`, `decal.rs`, `ground.rs` | **Bridge/support.** Linear/sRGB conversion, PNG data/cache, fallback sheets, decal/ground metadata. Alpha and numeric masks must not be treated as sRGB colour. Ground material also serves non-lighting gameplay/audio. |
| `src/engine/gltf.rs`, `src/engine/props.rs` | **Bridge/Data contract.** GLB material emission/strength, textures/alpha, normals, UV response, transformed/skinned geometry, model admission and shared asset caches. Lighting consumes these; animation and collision also depend on them. |
| `src/engine/quality.rs` | **Core policy.** Static versus Full lightmaps, reflection and graphics profiles, independent texture/lighting quality controls and action routing. |

## 3. CPU source field, transport, atlases and irradiance

All paths in this table are under **`src/engine/lighting/`**, except the facade
`src/engine/lighting.rs`. This table covers every file in that directory.

| File/group | Role and key connections |
| --- | --- |
| `../lighting.rs` | **Core facade.** Public source/field APIs, exports, shared preparation ownership and the conceptual baseline/local-pool model. |
| `light.rs`, `color.rs`, `math.rs`, `tuning.rs` | **Core.** Source shapes, RGB emission, falloff/sanitized scalar maths, calibration and fixture-family profiles. Shared by loader, editor, geometry and live source handling. |
| `bake.rs` | **Core + serialized contract.** `LevelLighting`, room/partition baselines, openings/storeys, source resolution, switchable fields, sampling and compiled record encoding/decoding. A large mixed computation/format owner. |
| `visibility.rs` | **Core.** Static walls/floors/ceilings/openings visibility and compiled spatial boundaries. Must agree with actual collision/architecture openings. |
| `occlusion.rs` | **Core/support.** Conservative prop occlusion geometry from models for baseline/pool visibility; distinct from detailed transport triangles and GPU character shadows. |
| `directional.rs`, `directional_index.rs` | **Core.** Map-wide directional sources and conservative rejection acceleration with exact leaf tests. |
| `transport.rs` | **Core.** Offline linear-HDR direct sampling, deterministic diffuse bounce solve, receiver energy and directional moments, solver configuration/fingerprint. No ordinary player solve. |
| `transport/alpha.rs`, `coverage.rs` | **Core.** Authored alpha-ray transmission and physical visibility-footprint integration across triangulation/chart cuts. |
| `transport/bvh.rs`, `surface_cache.rs` | **Core.** Ray acceleration and previous-bounce irradiance sampling at actual hit surface coordinates. |
| `transport/batch.rs`, `filter.rs` | **Core.** Shared immutable geometric queries for light layers and diffuse denoising across physical/chart boundaries. |
| `transport/runtime_visibility.rs` | **Core bridge.** Reuses immutable opaque scene geometry for bounded live segment queries, without changing baked lighting. |
| `transport/diagnostics.rs`, `transport/diagnostics/cpu.rs`, `transport/diagnostics/gpu_oracle.rs`, `transport/probe_audit.rs` | **Check.** Offline dumps, traversal/gather microkernels, CPU certificates for the explicit GPU experiment and probe audits. Diagnostic output is not the production solve. |
| `lightmap/mod.rs` | **Core/Data contract.** Charts/patches/configuration, HDR irradiance and first-moment texels, page/layer types, validity and fill outcomes. |
| `lightmap/plan.rs`, `lightmap/atlas.rs` | **Core.** Geometry-stamped chart allocation, deterministic atlas placement, page storage and diagnostic image output. |
| `lightmap/cache.rs` | **Bridge.** Content keys and transient cache; source/material/model/solver identity changes must invalidate it. |
| `probes.rs`, `probe_placement.rs` | **Core + serialized contract.** Prepared 3D irradiance field, interpolation/room labels, direct and switchable contributions, bounded binary reader/writer and valid air-volume placement. These are diffuse irradiance probes, not reflection cubemaps. |

## 4. Renderer-neutral geometry, moving receivers and effects

Paths below are under **`src/engine/render/common/`**.

| File/group | Role and key connections |
| --- | --- |
| `api.rs`, `light_transport.rs` | **Core/Bridge.** Build/fill orchestration and conversion of rendered architecture/props/materials into transport receivers, albedo, emitters, alpha and water geometry; prepares immutable runtime visibility where required. |
| `geometry.rs`, `architecture.rs`, `structure.rs`, `contact.rs` | **Core/Bridge.** Lightmapped surface emission, structural backing and exact contact subtraction before charts. A geometry rewrite must preserve visible/collision/occlusion agreement. |
| `fixtures.rs` | **Core/Bridge.** Visible fixture geometry and luminous faces from the shared fixture profile/catalog. |
| `mesh.rs`, `prop_vertices.rs`, `canonical_props.rs` | **Bridge/Data layout.** Vertex/mesh wire layout, packed lightmap fields, neutral caustic factors, exact prop attributes and safe runtime geometry sharing. Shared shapes must not merge distinct lighting. |
| `props.rs` | **Core/Bridge.** Static model placement, material/emission routing, baked vertex illumination, lightmap receiver ownership and caster classification. |
| `dynamic.rs`, `character.rs` | **Core/Bridge.** Moving object/character receiver sampling and direct-light support, normal/pose/socket state, shadow ownership and stable probe anchors. Also large animation/scene owners; preserve their unrelated work. |
| `dynamic_visibility.rs` | **Core.** Model-local rays and PNG alpha resources; updates transforms/revisions without rebuilding stationary geometry. |
| `dynamic_lights.rs` | **Core.** Bounded attached/glow/spot lights, direct-source selection and live uniform records; connected to entity cues and irradiance decomposition. |
| `blacklight.rs` | **Core.** UV-source selection, source visibility and pigment excitation. UV is kept separate from ordinary visible transport. |
| `normal.rs`, `posed_normals.rs` | **Core/support.** Shared normal transforms and recomputation for deforming cavity geometry. Normal errors affect lighting even with a correct solver. |
| `reflections.rs`, `probe_filter.rs` | **Core.** Reflection planes/probe placement and roughness mip filtering. Distinct from diffuse `lighting/probes.rs`. |
| `materials.rs`, `animation.rs` | **Core.** Draw material state and time-dependent emission/pulse/flicker policy. |
| `atmosphere.rs`, `storm.rs`, `postprocess.rs` | **Core appearance.** Ordinary distance/volume fog, shelter-aware storm optical depth and bloom/exposure/tone/grade settings. |
| `water.rs`, `doors.rs`, `elevators.rs` | **Bridge/receivers/casters.** Shared surface/transformed geometry enters visibility and lighting, while simulation owns motion. |
| `decals.rs`, `effects.rs`, `snow.rs`, `blood.rs` | **Consumers/presentation.** Surface markings and particles need consistent lighting/fog/pass routing. Steam/snow are not static bake occluders; blood presentation does not introduce sources. |
| `occlusion.rs` | **Adjacent, not light transport.** Current-view geometry culling; “occlusion” here decides visibility to the camera. |
| `camera.rs`, `view.rs`, `framebuffer.rs`, `stats.rs`, `preview_cache.rs`, `mod.rs` | **Support/Bridge.** View maths, target sizes, measurements, preview reuse and exports. Not independent lighting solvers. |

`src/engine/render.rs` and `src/engine/render/facade.rs` expose these types and
backend operations to game/editor callers. Preserve their public contract or
update all consumers deliberately.

## 5. GPU passes and every production WGSL file

Paths below are under **`src/engine/render/wgpu/`**. Production shader fragments
are composed in `world.rs`, `decals.rs`, `posed_visibility.rs` and
`posed_ambient.rs`; search their `include_str!`/`concat!` calls before moving files.

| File/group | Role and key connections |
| --- | --- |
| `renderer.rs`, `mod.rs` | **Core orchestration/exports.** Resource lifecycle, per-frame updates, source states, pass order and settings changes. |
| `world.rs`, `world.wgsl` | **Core.** Shared architecture/prop/character pipelines, packed vertex interpretation, lightmap reconstruction, moving-receiver irradiance/direct shading, materials, emission and reflection combination. |
| `environment.rs`, `lightmap.rs` | **Core/Bridge.** Bind group/uniform layout, layer addressing, switch weights, fog/reflection inputs and HDR atlas GPU upload. Keep Rust structs and shader layouts synchronized. |
| `lighting_visibility.rs`, `attached_lights.wgsl`, `flashlight.rs` | **Core.** Shared immutable visibility upload, attached/spot-light shading and flashlight depth visibility target. This is separate from offline lightmap shadows. |
| `posed_topology.rs`, `posed_visibility.rs`, `posed_queries.wgsl` | **Core.** Character opaque topology, pose refit and ray traversal for receiver-owned practical-light shadows. |
| `posed_ambient.rs`, `posed_ambient.wgsl` | **Core.** Current-pose scalar accessibility with separate front/back handling; does not replace the ordinary diffuse irradiance field. |
| `posed_depth.rs`, `posed_depth.wgsl` | **Core.** Per-receiver/emitter-tap depth layers and projection support; unsupported cases retain ray-query handling. |
| `character.rs`, `character_blend.rs`, `character_transparency.rs` | **Core/Bridge.** Per-character environments, posed visibility/ambient resources, opaque/cutout/BLEND routing and sorting. Transparency is not simply another shadow toggle. |
| `dynamic.rs`, `props.rs`, `props/staged.rs`, `prop_instances.rs`, `prop_instances.wgsl` | **Core/Bridge.** Moving and instanced rigid receivers, per-instance environments/materials, GPU upload and shared world-shader specialization. |
| `skinning.rs`, `skinning.wgsl` | **Bridge.** GPU position/normal deformation for all views. Preserves the packed lighting/vertex attributes used by lighting and shadows. |
| `caustics.rs`, `caustics_field.rs`, `caustics_receiver.rs`, `caustics.wgsl` | **Core.** Source-resolved underwater illumination, geometry-only receiver lattice, blocker revisions and self-excluded moving-receiver support. Pattern imagery is a committed PNG. |
| `uv_receivers.rs` | **Core/Bridge.** GPU receiver subdivision/sampling geometry for fluorescent architecture without changing saved atlas charts. |
| `reflections.rs` | **Core.** Reflection cubemap upload/capture and planar targets/passes; shares scene/material/lighting state with the main view. |
| `material.rs`, `texture.rs`, `texture/model_keys.rs`, `texture/worker.rs` | **Core/Bridge.** GPU material uniforms and normal/emission/mask bindings, colour-space-correct texture uploads/mips, image identities and staged work. |
| `postprocess.rs`, `post.wgsl` | **Core.** HDR targets, emission/bloom extraction, footprint prefilter/blur, exposure/tone/grade and final display transfer. A bright/dark bug may originate here rather than in transport. |
| `atmosphere.wgsl`, `storm.wgsl` | **Core appearance.** Shared fog integration and shelter-aware weather extinction applied by multiple material passes. |
| `sky.rs`, `sky/loading.rs`, `sky.wgsl` | **Core appearance/Bridge.** Sky and celestial visuals, PNG decode/mips and view-direction sampling; coordinate with directional lighting rather than assuming the image emits light. |
| `water.rs`, `water.wgsl` | **Core appearance/consumer.** Surface normals, depth/colour transfer, transparency and water-specific material branches. Caustics have their own files above. |
| `decals.rs`, `decals/staged.rs`, `decals.wgsl` | **Consumer/Core shading.** Decal lighting, UV, fog and caustics combination with distinct alpha/depth behavior. |
| `effects.rs`, `effects.wgsl` | **Consumer/presentation.** Billboards and alpha compositing into the scene; preserve their HDR/fog interfaces. |
| `ui.rs`, `ui.wgsl`, `tool_view.rs` | **Explicit boundaries.** UI uses a neutral display-space path. First-person tool meshes have private depth and do not become ordinary world shadow/reflection casters. |
| `surface.rs`, `startup.rs` | **Bridge.** Surface colour formats/presentation and pipeline preparation; changes can alter perceived brightness or startup behavior without changing lighting equations. |
| `diagnostics.rs`, `diagnostics.wgsl`, `probe_visualization.rs`, `character_audit.rs` | **Check.** Diagnostic decomposition/overlays and explicit GPU/CPU character comparisons; diagnostic shader is feature-gated. |
| `bloom_probe.rs`, `postprocess_probe.rs`, `readback.rs`, `gpu_timing.rs` | **Check/support.** Numeric bloom/display controls, framebuffer capture and real submitted-pass timing. Preserve useful independent oracles during a rewrite. |

## 6. Gameplay, live light control and saved state

| File/group | Role and connections |
| --- | --- |
| `src/engine/entities/mod.rs`, `src/engine/entities/components.rs` | **Core/Bridge.** Authored lights become entities; light toggles, flicker/emission states, light-response behavior, glow cues and render frames. A rewrite must preserve gameplay-facing IDs/events. |
| `src/engine/entities/light_power.rs` | **Core control.** Circuit/switch power composes with requested light/flicker state rather than rebaking the world. |
| `src/engine/entities/checkpoint.rs` | **Bridge/Data contract.** Saves/restores circuit power and requested state. |
| `src/engine/entity.rs`, `src/engine/entities/spawn.rs`, `src/engine/entities/guest_lifecycle.rs` | **Bridge.** Renderable source/glow/shadow-only cues, spawned caster ownership and lifecycle cleanup. |
| `src/engine/entities/balloon.rs`, `src/engine/entities/encounter.rs` | **Consumers.** Stable moving lighting anchors and character render-state creation. Preserve gameplay/physics independently. |
| `src/engine/ai/mod.rs` | **Consumer.** Light-response behavior; not an illumination solver. |
| `src/engine/feltling.rs`, `src/engine/tall_guest.rs` | **Consumers.** Character ownership/admission checks include shadow-only and authored render-state restrictions; these files primarily own creature behavior. |
| `src/engine/physics/config.rs`, `src/engine/physics/world.rs`, `src/engine/level/runtime_collision/physics.rs` | **Shared-geometry boundary.** Dynamic placements must stay out of static lighting batches, while physical surfaces must not be subdivided according to baked lighting cells. These are physics owners, not light solvers. |
| `src/engine/scripting/api.rs`, `src/engine/entities/events.rs`, `src/engine/entities/sequence_runtime.rs`, `src/engine/entities/sequences.rs` | **Bridge/API.** Script `light` operations, `SetLight` actions, event/sequence routing and visibility/state queries. |
| `src/game/app/flashlight.rs` | **Core control/Bridge.** Player flashlight state and renderer spotlight update. |
| `src/game/settings.rs`, `src/game/ui.rs`, `src/game/ui/menu.rs`, `src/game/app/actions.rs` | **Authoring/runtime control.** Persisted graphics choices, menu/settings rows and actions for lightmaps, reflections, self-shadows, bloom and diagnostic controls. |
| `src/game/app/item_runtime.rs`, `src/game/app/carry_runtime.rs`, `src/game/pickup_catalog.rs` | **Consumers/Bridge.** Item render ownership, lighting-related action dispatch/diagnostics and tool/item definitions. |
| `src/game/audio.rs` | **Consumer.** Fixture-family and powered-light audio cues. Keep hum/audio behavior when moving fixture metadata. |
| `src/game/bench.rs`, `src/game/bench/lighting_sequence.rs`, `src/engine/logging.rs` | **Check/Bridge.** Graphics overrides, fixture/action replay, lighting timings and logged counters. |

Authored Lua consumers found in `projects/menu/scripts/menu.luau`,
`projects/beam/scripts/{beam_1,beam_office_1,beam_pool_1,beam_arcade_1}.luau`, and
`projects/environments/scripts/lantern_hollow.luau` also need review. Some matches
are descriptive text, but script-visible light controls are a compatibility API.

## 7. Editor ownership

| File/group | Role and connections |
| --- | --- |
| `src/editor/lighting.rs` | **Authoring.** Circuit membership/switch assignment, light painting and local lighting views. |
| `src/editor/flicker.rs`, `src/editor/document/flicker.rs`, `src/editor/document/circuits.rs` | **Authoring.** Editable flicker/circuit transactions, switch assignment and required-Full metadata. |
| `src/editor/document/fog.rs`, `src/editor/document/gameplay/shadow_only.rs`, `src/editor/document/gameplay/properties.rs`, `src/editor/document/gameplay.rs` | **Authoring.** Fog volumes, shadow-only validation and property edits; save/undo compatibility matters. |
| `src/editor/document/components.rs`, `src/editor/document/components/values.rs`, `src/editor/document/mounting.rs`, `src/editor/authoring.rs`, `src/editor/stairs.rs` | **Authoring/Bridge.** Field values, fixture transforms/mounting, source defaults and utility placement. |
| `src/editor/ui/components.rs`, `src/editor/ui/components/fields.rs`, `src/editor/ui/inspector.rs`, `src/editor/ui/gameplay/sheets.rs` | **Authoring/UI.** Lighting controls, numeric ranges, glow/shadow properties and inspector presentation. |
| `src/editor/ui.rs`, `src/editor/ui/dock.rs`, `src/editor/ui/library.rs`, `src/editor/ui/workspace.rs`, `src/editor/browser.rs`, `src/editor/actions.rs`, `src/editor/mod.rs`, `src/editor/safety_actions.rs` | **Bridge/UI.** Lighting tab/tool entry points, actions, popup/selection state and cancellation. |
| `src/editor/viewport.rs`, `src/editor/viewport/picking.rs` | **Authoring/consumer.** Editor renderer setup, source gizmos and picking using resolved fixture geometry. |
| `src/editor/viewport/preview.rs`, `src/editor/viewport/preview/local.rs`, `reuse.rs`, `audit.rs` | **Core preview/Check.** Neutral approximate previews, scoped lighting work, geometry/resource reuse and equality/audit measurements. Local preview must not silently alter final packages. |
| `src/editor/bakes.rs`, `src/editor/ui/bake_progress.rs` | **Bridge.** Final build/export orchestration, cancellation, progress and lighting-cache inspection/removal. |
| `src/editor/test_prepare.rs`, `src/editor/test_session.rs`, `src/editor/viewport/test_runtime.rs`, `src/editor/play_here.rs`, `src/editor/ui/runtime.rs` | **Bridge.** Unbaked in-editor test snapshots; their approximate lighting is not final Full-lighting acceptance. |
| `src/editor/reuse/prefabs.rs`, `src/editor/verification.rs` | **Authoring/Check.** Prefab requirements and native automation receipts for actual editor state. |
| `src/editor/document.rs`, `src/editor/document/persistence.rs`, `src/editor/document/project.rs` | **Persistence boundary.** Generic document history/save/load must retain lighting fields even if ownership is consolidated. |

Future Codex level creation/change/build work must use the actual in-game editor
as required by `AGENTS.md`. Existing JSON examples or legacy authoring scripts
listed here are references, not permission to bypass that rule.

## 8. Compiler, packages, caches and version boundaries

| File/group | Role and connections |
| --- | --- |
| `src/map_compiler/mod.rs`, `application.rs`, `gui.rs`, `events.rs`, `progress.rs` | **Bridge/orchestration.** Compile/verify/export entry points, Static/Full selection, cancellation, stage events and GUI progress. |
| `src/map_compiler/probes.rs` | **Core preparation.** Room/air-volume placement labels for the irradiance field. Reflection preparation also involves renderer/common and wgpu, not this file alone. |
| `src/map_compiler/diagnostics.rs` | **Check.** Read-only lighting exports, stage provenance and diagnostic artifacts. |
| `src/map_compiler/storage.rs`, `distribution.rs`, `project_spool.rs`, `geometry_check.rs` | **Bridge.** Lightmap storage optimization, multi-level source/project publication, spill staging and receiver/geometry admission. Geometry checks are shared beyond lighting. |
| `src/engine/package/lighting.rs` | **Data contract.** Bounded `LevelLighting` archive entry; private layout is encoded in `lighting/bake.rs` and `visibility.rs`. |
| `src/engine/package/lightmaps.rs`, `compact_lightmaps.rs`, `ktx2.rs` | **Data contract.** Atlas/chart/layer metadata, compact/constant tile storage and KTX2 HDR plane/cubemap containers. |
| `src/engine/package/mesh.rs`, `props.rs`, `shared_props.rs`, `shared_props/staging.rs` | **Data contract.** Packed geometry contains lightmap and caustic attributes, caster ownership and factored vertex planes. |
| `src/engine/package/world.rs`, `manifest.rs`, `mod.rs` | **Data contract/loader.** Variant entries, irradiance and reflection payloads, size limits, capability admission and prepared-world installation. |
| `src/engine/package/catalog_inputs.rs`, `dependencies.rs`, `hash.rs`, `src/engine/canonical_json.rs`, `build.rs` | **Identity/invalidation.** Catalog/material/fixture dependencies, canonical source hashes and compiler identity. `build.rs` hashes engine/compiler source, Cargo/toolchain inputs and selected committed assets. |
| `src/engine/package/project.rs`, `semantics.rs`, `release.rs`, `src/engine/project/folder.rs` | **Saved-content boundary.** Recoverable authored maps, source validation and project/runtime release compatibility; preserve editable lighting rules and map-owned materials. |
| `src/engine/package/archive.rs`, `archive/`, `binary.rs` | **Infrastructure.** Bounded reads, compression and binary primitives used by lighting payloads and many unrelated systems. |

Reflection captures are a specific platform boundary: `CaptureContext` in
`src/map_compiler/mod.rs` lazily creates the headless renderer when a Full variant
has reflection routing points. `src/engine/render/wgpu/renderer.rs` requests the
native graphics adapter for that capture. CPU transport alone does not make every
map build GPU-independent. Keep that requirement visible in build/release tooling;
do not silently remove required captures to make an unsupported host succeed.

Current source declares map package major **2**, lighting record **1**, lightmap
record/cache format **1**, irradiance field record **1**, and transport
`SOLVER_REVISION = 8`. These are different identities, not interchangeable version
numbers. Check their definitions again before migration. Reflection positions,
mesh attributes, project archives and individual capabilities have additional
contracts in the files above. A changed solver needs deliberate invalidation;
a changed record layout needs a reader/version/migration policy. Do not merely
accept old bytes with new equations.

## 9. Assets and map data that affect lighting

| File/group | Role and risk |
| --- | --- |
| `assets/catalog.json` | **Authored data.** Current catalog has 688 entries. Fixture/model IDs, material emission, normal/response/reflection/UV metadata and referenced PNG/GLB paths are shared across maps. |
| `assets/completed-models.json` | **Asset admission data.** Reviewed model hashes and contracts; changing rig/geometry can invalidate shadow/cavity mappings even if vertex counts match. |
| `assets/entities/tall_guest/model/cavity_visibility.bin` | **Generated numeric data, shipped.** Fixed reviewed cavity accessibility embedded by `common/character.rs::baked_cavity`; not a texture or general-purpose body shadow solution. |
| `assets/environments/*/textures/lights/` | **Authored PNG imagery.** Office/arcade fluorescent faces, blacklight tube, Home ceiling light, pool round/wall lamps and Backrooms fixture faces. Preserve UV/aspect/alpha contracts. |
| `assets/shared/textures/normals/`, `assets/shared/textures/effects/caustics_01.png` | **Authored PNG data.** Normal fields and caustics image; numeric normals/masks and colour textures have different sampling rules. |
| `assets/environments/{outdoor,winter,beach,frutiger_aero}/textures/sky/` | **Authored sky PNGs.** Visual environment/colour context; not automatically physical emitters. |
| `assets/environments/winter/props/string_lights*`, `assets/environments/arcade/props/blacklight_fixture.glb`, `assets/environments/pool/props/pool_light_*.glb`, `assets/environments/home/props/ball_light*`, `assets/environments/outdoor/props/streetlight*`, `assets/environments/frutiger_aero/props/light_pod.glb` | **Authored model/material data.** Explicit visible light fixtures and luminous props. Their attached source definitions are separate from emissive materials. |
| All other `assets/**/*.glb` and runtime material PNGs | **Broad inputs, not all lighting code.** Geometry/normals, albedo, cutout/blended alpha, emissive textures and UV masks affect occlusion and transport. Filenames alone cannot identify all emitters; GLB material data is embedded. |
| `projects/**/levels/*.json`, `projects/**/places.project.json`, project-owned materials/scripts/assets | **User/source contracts.** Actual light placements, power/flicker, post/fog/sky/water settings and material overrides. Keep authored intent and recoverable sources when replacing computed outputs. |
| `assets/shared/menu/scene.bin`, external `.placesmap` files, generated lighting/cache directories | **Generated/prepared data.** Compiled menu and maps embed lighting from a particular compiler/source/platform. Regenerate through the approved workflow when identities change; do not hand-edit payloads. |
| `archive/projects/`, `docs/archive/levels/`, `levels/`, ignored local projects/captures | **Historical/developer/user data.** Inventory references, not automatically current release content or safe deletion candidates. |

## 10. Tools, tests and independent evidence

### Lighting-focused developer tools

| File/group | Purpose |
| --- | --- |
| `tools/bench/lightmap_report.py`, `inspect_lighting_dump.py`, `probe_lighting_report.py`, `lighting_receivers.py` | **Check.** Atlas/dump/probe/receiver analysis. Keep their expected record/layout assumptions synchronized. |
| `tools/bench/capture_lighting_quality.py`, `capture_lighting_regions.py`, `lighting_quality_views.json`, `colour_response_control.py` | **Check.** Native comparison captures and colour/region controls. |
| `tools/bench/flashlight.py`, `caustics_sources.py`, `elevator_light.py`, `audit_winter_strings.py` | **Check.** Targeted dynamic-source, water, moving-room and string-light behavior. |
| `tools/bench/check_character_capture.py`, `character_transparency_inputs.py`, `check_quality_regression.py` | **Check.** Character actual-draw evidence, transparency and settings comparisons. |
| `tools/rust/lighting-gpu-probe/{main.rs,device.rs,adversarial.rs,rays.wgsl,README.md}` | **Explicit experiment/Check.** GPU ray microbenchmark and CPU oracle; not the production transport solver. |
| `tools/entities/bake_tall_guest_cavity.py`, `validate_entities.py`, `render_contact_sheets.py` | **Offline authoring/Check.** Exact model cavity data and model/pose visual contracts. |
| `tools/authoring/shadow_only_editor.py`, `shadow_only_native.py`, `fixtures/shadow_default_pose.json` | **Check.** Actual editor and native shadow-only behavior. |
| `tools/levels/build_lighting_quality.py`, `build_fixture_levels.py`, `build_capacity_fixtures.py`, `build_model_zoo.py` | **Legacy fixture authoring/Check.** Existing source generation references, not production runtime and not authorization for future direct level-file editing. |
| `tools/textures/lights_art.py`, `tools/props/string_lights.py`, `tools/props/parts/string_lights.py`, `tools/props/glb.py`, `tools/assets/validate.py`, `audit.py` | **Asset authoring/Check.** Luminous imagery/model materials/source metadata and asset validity. Other prop/texture authoring families can also encode emission/normal/alpha; preserve the PNG-only runtime asset policy. |
| `tools/bench/compiler_bench.py`, `compiler_incremental.py`, `compare_packages.py`, `map_storage.py`, `package_storage.py`, `tools/rust/package_repack/` | **Check/infrastructure.** Compiler cost/currentness, packaging and lossless storage comparisons that include lighting. |
| `tools/verify.sh`, `tools/check-rust.sh`, `.github/workflows/rust.yml`, `tools/verify-menu.py`, `tools/package*.py`, `tools/package.sh` | **Integration gates.** Compile/tests, prepared menu and map-package integrity. Not lighting algorithms. |
| `.github/workflows/release.yml`, `tools/release/hosted.py` | **Build integration.** Hosted application packaging prepares the bundled menu and verifies its lighting/source currency with the matching platform compiler. Preserve the required reflection-capture dependency when changing this path. |

Other `tools/bench/` and `tools/authoring/` capture/acceptance scripts often set
lightmap/reflection/bloom flags or replay fixture changes. Retain them as consumer
and performance evidence; do not count every mention of “lighting” as a solver.
Likewise `tools/props/parts/` and `tools/textures/` contain offline material/image
authoring; their entire trees should not be deleted as a lighting rewrite.

### Rust regressions

Tests live under their owning `tests/` modules. Preserve or deliberately replace
their behavior-level assertions, rather than carrying over implementation-shaped
tests just to preserve test counts.

| Owning directory / files | Behaviors covered |
| --- | --- |
| `src/engine/tests/transport_{solver,emission,filter,probes,surface_cache}.rs`, `directional_index.rs`, `celestial_sources.rs` | Direct/bounce transport, emission separation, filtering, probe solve, hit-coordinate lookup and directional sources. |
| `src/engine/tests/lightmap_limits.rs`, `compact_lightmap_planes.rs`, `constant_lightmap_tiles.rs`, `compiled_sections.rs`, `archive_budget.rs`, `package_loading.rs`, `project_package.rs` | Bounds, numeric/storage contracts, compiled records and package admission/recovery. |
| `src/engine/tests/lighting_receivers.rs`, `probe_visibility.rs`, `rigid_instance_lighting.rs`, `light_regions.rs`, `contact_shadow.rs`, `trim_contact.rs`, `elevator_light.rs` | Receiver ownership, spatial/room isolation, contact and moving geometry. |
| `src/engine/tests/caustics_{albedo,field,metadata,receiver_floor}.rs`, `water_surface_cache.rs`, `fog_volumes.rs` | Water source/receiver correctness, neutral albedo, visibility and fog contracts. |
| `src/engine/tests/static_ray_cache.rs`, `dynamic_ray_bounds.rs`, `gpu_probe.rs`, `dynamic_mesh_admission.rs` | Ray certificates, bounded dynamic queries, GPU oracle and mesh admission. |
| `src/engine/tests/character_{ownership,preparation,blend,transparency}.rs`, `prop_power.rs`, `preview_neutral.rs`, `canonical_props.rs`, `shared_props_attributes.rs`, `prop_vertex_storage.rs`, `prop_upload_bounds.rs` | Character/static caster separation, per-instance lighting/material data, power states, preview neutrality and packed attributes. |
| `src/editor/tests/{circuits,flicker,utility_lights,stairs_lights,fog,components,mounting,preview_reuse}.rs` | Editor authoring, source preservation and preview behavior. |
| `src/game/tests/{settings,menu,checkpoints,elevators}.rs` | Graphics preference/UI persistence, circuit checkpoint restoration and moving-room integration. |
| `src/map_compiler/tests/{project_archive,project_spool,geometry_surfaces,water,navigation,navigation_cache,scripting}.rs` | Lighting-bearing project exports and shared geometry/content dependencies; several are primarily non-lighting tests. |

`src/engine/tests/mod.rs`, `src/editor/tests/mod.rs`, `src/game/tests/mod.rs` and
`src/map_compiler/tests/mod.rs` register coverage. Removing a file or declaration
can silently remove a regression if test discovery is not checked.

Additional engine test consumers found by the symbol scan are
`src/engine/tests/{acoustics,animated_door,gameplay_authoring,level_import,native_interaction,navigation_ai,project_contract,ragdoll,ramp_mesh,settled_interpolation,shared_ai,stacked_item_clearance,tall_guest_player,tall_guest_stairs}.rs`.
Their fixtures or assertions touch shared lighting/geometry/character state;
they primarily protect other systems and must not be removed merely because a
lighting dependency is renamed. `src/editor/navigation.rs` explicitly prepares
navigation without a lighting bake and is another boundary to preserve.

### Python fixtures, native fixtures and documentation

- `tests/test_lightmap_harness.py`, `test_lighting_regions.py`,
  `test_lighting_dump.py`, `test_probe_lighting_report.py`: diagnostic contracts.
- `tests/test_glb_materials.py`, `test_string_lights.py`, `test_wgpu_bootstrap.py`,
  `test_quality_regression.py`, `test_character_capture.py`, `test_package.py`,
  `test_release_levels.py`: material/shader, source/quality, draw and archive gates.
- `tests/fixtures/levels/lighting_{contact_walls,isolation,quality,repair_cases,diagnostic}.json`,
  `elevator_light_screen.json`, `tests/fixtures/native/model-lighting/`,
  `tests/fixtures/native/birthday-light-response*.json`: small controls/capture recipes.
- `docs/RENDERER.md`, `docs/RENDERER_REFERENCE.md`, `docs/MAP_AUTHORING_GUIDE.md`,
  `docs/ASSET_SPECIFICATION.md`, `docs/PACKAGE_FORMAT.md`, `docs/PROJECT_FORMAT.md`,
  `docs/VERIFICATION.md`, `docs/assets/SHADOWS.md`: intended contracts and workflows.
- `docs/assets/shadow-inventory-2026-11-07.json`, `docs/art-style/stage7/`,
  `docs/reports/{runtime-lighting-0.13.1-mac,model-shadows-0.13.1-mac,elevator-light-0.13.1-mac}.md`,
  other lighting/colour/caustics/storage reports and `docs/images/`: dated evidence,
  not authoritative current source or permission to reuse an obsolete format.
- `local/verification/` and `local/release-*/` contain ignored unique captures,
  logs, binaries and receipts. They are not part of the production implementation;
  do not delete them with build caches or treat them as maintained source.

## 11. Boundaries to preserve before any replacement

1. **Saved content:** light IDs, source shapes/units, switching, prop emission,
   material IDs, fog/sky/caustics settings and project source recovery. Decide an
   explicit compatibility/migration policy before changing any serialized type.
2. **Prepared content:** atlas/chart/moment layout, irradiance fields, reflection
   records, capability flags and source/asset/compiler fingerprints must change
   together. A package can parse successfully and still be physically wrong.
3. **Geometry and visibility:** walls/openings/floor slabs, contact subtraction,
   collision, model alpha and sidedness must agree. Keep camera culling distinct
   from light occlusion, physics water coverage and source shadowing.
4. **Energy and colour:** preserve the separation of albedo, direct/indirect
   illumination, emission, UV excitation and post exposure. Preserve linear HDR
   accumulation and one correct final display transfer across reflection views,
   decals, water, transparent characters and UI.
5. **Moving objects:** rigid/posed receiver ownership, shadow-only objects, carried
   props, elevators, sockets/glows and first-person tool exclusions. The authored
   Tall Guest cavity mapping is tied to reviewed geometry, not vertex count alone.
6. **Behavior:** switches, flicker, scripts, light-response AI, checkpoints,
   reset/despawn and editor undo/save. A renderer rewrite must not silently change
   game rules or lose source settings.
7. **Work/memory limits:** light/probe/chart/page counts, shader binding sizes,
   dispatch limits, bounded package reads, cancellation and worker ownership.
   The existing crowded opt-in posed-shadow workgroup limit is a known acceptance
   concern; do not relabel it fixed by reorganizing files.
8. **Acceptance:** keep simple lit/unlit isolation controls, source toggles,
   contact/alpha/posed cases, native view comparisons and corrupt-package tests.
   Native and performance campaigns stay explicit; ordinary CI must not require
   a full shipped-map bake. Throughput comparisons must disable pacing and record
   actual presentation mode/drawable size as required by `AGENTS.md`.

Potential consolidation boundaries are authored lighting data, offline
preparation, shared visibility, runtime receivers/sources, GPU material evaluation,
post/atmosphere, serialization, and editor integration. This is an ownership map,
not a recommendation to combine all responsibilities into one file.

## 12. Search method and exclusions

The inventory was checked against directory/module declarations, shader
`include_str!` composition, public lighting symbols, package readers/writers,
editor controls, test registration and asset references. Broad lexical passes
found **266 Rust/WGSL source files**, **141 tool source files** and **24 Python
test files** before symbol/path expansion and false-positive classification.
These counts describe candidate matches, not counts of lighting algorithms.

Re-run these read-only searches after later refactors:

```sh
rg --files src tools tests assets docs | rg -i 'light|shadow|probe|bake|emiss|caustic|fog|sky|reflect|normal'
rg -l -i 'lightmap|irradiance|shadow|lighting|emissive|tonemap|caustic|blacklight|reflection_probe|bloom|light_power' src tools tests
rg -l 'LightSource|LightFixture|LightDef|LevelLighting|SetLight|LightResponse|Flicker|GlowCue|LightmapQuality|MaterialEmission|MaterialResponse|FogDef|SkyDef|PostProcess|set_light|light_enabled|static_light' src
rg -n 'include_str!|include_bytes!' src/engine/render src/engine/materials src/engine/props.rs
rg -n 'VERSION|REVISION|fingerprint|capabilit' src/engine/lighting src/engine/package src/map_compiler build.rs
rg -l -i 'light|flicker|glow|fog' projects --glob '*.luau'
```

Searches alone are insufficient: GLB material data is binary, arbitrary map-owned
materials/scripts can affect light behavior, and generic helpers often lack
lighting terms. The coherent asset/map groups above deliberately cover those
inputs without claiming every PNG is a light or every library is lighting code.

Examples excluded from “lighting algorithm” classification: `movement_probe.rs`
is a movement diagnostic; “ambient” audio assets/`tools/audio/tonal.py` are sound;
`src/engine/physics/water.rs` uses “shadowed” for overlapping water volumes;
`src/engine/loader/validation/entities.rs` can use “shadows” for identifier
collisions; `render/common/occlusion.rs` culls camera-visible objects. Physics,
navigation, acoustics and animation files may consume shared geometry or light
state, but are not wholesale lighting deletion targets.

External `wgpu`/Naga, SDL, math/image/compression dependencies are declared in
`Cargo.toml`/`Cargo.lock`; Cargo registry sources and `target/` products are
generated/downloaded dependencies, not Places-owned lighting source. The explicit
GPU probe tool is an experiment, not a second production backend. Historical
GLSL references in documentation/comments are not active shader programs.
