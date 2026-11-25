# Safe asset refresh in Places

**Recommendation — keep the current workflow and prototype one manual PNG
refresh separately, if approved.** Places already rereads source textures when
it prepares a view. The missing contract is that a broken replacement must not
displace a usable view. Borrow Bevy's separation of asset identity, dependency
invalidation and render preparation without adopting Bevy or adding a watcher.
[Issue #46][issue]

Source review of Places [`78bbbf21444f1caf710ac403deb176387c3add6a`][places]
(`origin/0.14.0-dev`) and official Bevy [v0.20.1][release] at
[`a0577b5c6a7b208719dd761c5c6a7a539e472a97`][bevy].
**Performance unmeasured.** No build, prototype, native run or filesystem
scenario was performed on Mac, Windows or Linux.

## What a Places preview request actually does

- **A save to an asset file does not schedule a request.** Opening a project,
  committing a scene edit, undo/redo, changing isolation or visible layers, and
  toggling Inspect/Authored do. Draft previews and scoped Local lighting also
  request preparation. Save validates and writes authoring data, but does not
  itself call the viewport rebuild. Toggling Inspect/Authored already sees
  stable same-path edits to supported scene inputs without reopening the
  project. The request clones and, for the ordinary viewport, filters the
  current in-memory level. It prepares a whole authoring scene with reuse,
  then installs it with playback preservation off. Even when the map ID
  stays the same, this is not an in-place asset swap.
  [Open, edit and Save][editor] · [Undo/redo][history] · [Layers][layers] ·
  [Preview toggle][toggle] · [Draft and Local lighting][local] ·
  [Viewport request][view-request] · [Install call][view-install]
- **Current bytes, not timestamps.** The worker keeps a catalog, prop assets,
  decoded images and one previous build, held weakly for oversized builds.
  Each generation hashes the referenced placed/template/fixture model files
  and fitted character appearances with SHA-256 for the model/appearance
  input key. A changed key retains only successful models whose identities
  match and clears fitted appearances. The model load can read bytes later.
  Ordinary catalog material PNGs are read before decoded-cache lookup and
  keyed by logical ID plus a 64-bit hash of encoded bytes. Prefetch is
  bounded and the serial resolver rereads, so prefetch does not pin old
  pixels. Material definitions and resolved texture keys participate in
  scene reuse. Fixture sheets have their own resolution and file-backed
  decals have a separate upload path. This is not a single dependency cache.
  [Worker ownership][worker] · [Scene preparation][prepare] · [Dependency scan][model-scan] ·
  [Model cache][model-cache] · [PNG cache][png-cache] ·
  [Reuse keys][reuse] · [Fixture sheets][fixtures] · [Decals][decals]
- **The picker does not follow the scene.** The app and worker load separate
  catalog snapshots at startup. The browser uses its own search/choice
  cache, reads `editor/previews.json` lazily and caches both committed tile
  images and tile errors by ID until eviction or browser replacement.
  Toggling a view or reopening a project in the same editor does not
  recreate the browser. The UI compares image handles, not source file
  revisions. A changed wall PNG is not a regenerated thumbnail or a
  catalog change. Restart is the baseline when these session caches need
  to be recreated.
  [App startup][app-start] · [Browser and picker][browser] · [UI images][ui]
- **Latest request is not a disk snapshot.** There is one active CPU request
  and one replaceable pending request. A new request cancels the old budget
  cooperatively, and the UI discards responses bearing obsolete generations
  and cancels superseded GPU staging. These rules order requests, not disk
  writes. Model hashing and loading are separate. Material prefetch and
  resolution can see different writes, two consumers can reread the same
  path, and auxiliary artwork can resolve later. An external save alone
  does not advance a generation. The preview does not carry the compiler's
  per-operation read verification into these consumers.
  [Worker ordering][worker] · [Model scan][model-scan] ·
  [Live versus compiler reads][asset-reads] · [Verified compiler reads][verified-reads]
- **A rejected candidate and a malformed asset are different.** On the UI
  thread the renderer admits the candidate against resource limits, stages
  a new world and a cloned GPU cache, fits and uploads new content keys
  with their mips and bindings, and publishes after bounded upload work
  and fences complete. The candidate allows at most two upload units in
  flight. Unchanged textures can share handles, and eviction does not
  invalidate a resident draw's handles. A hard CPU failure or
  recoverable admission/upload failure leaves the resident render world
  installed and surfaces a diagnostic. Startup pipeline failure or
  device loss is not recoverable by retaining the old scene. A bad albedo
  or mask can instead resolve to a diagnostic material, a bad fixture sheet
  to an untextured face, and a bad model to a fallback or omission. Those
  are not necessarily rejected candidates.
  [Admission and errors][view-install] · [GPU staging][gpu-stage] ·
  [Upload fences][gpu-fences] · [GPU commit][gpu-commit] · [Texture keys and retirement][gpu-textures] ·
  [Material fallback][material-fallback] · [Model fallback][prop-fallback]
- **Old pixels do not mean the old view is fully usable.** A failed
  same-document preparation does not roll back unsaved edits, undo/redo
  history or the selected ID left by the initiating action. The action
  may itself change selection or history. The request clears the
  play/picking mesh and updates view guides before success. On failure,
  old GPU content and old bounds can remain visible while scene picking,
  wall tools and floor-based Play Here stay gated. Reopening a project is
  different. The app prompts for dirty work, then a successful document
  open replaces the document/history, clears selection and schedules
  reframing *before* the preview has completed.
  [Open and selection][editor] · [Dirty guard][actions] · [Document history][doc-history] ·
  [Viewport request][view-request] · [Readiness and picking][pick] ·
  [Picking publication][view-publish]

**Quick Test is a new session.** It prepares a private, unbaked copy of the full
level and current script buffers, then starts a simulation after installation.
An active test uses that snapshot, not the authoring viewport's request loop as
a live asset or script update mechanism. Stop and restart to pick up edits.
Ordinary Inspect/Authored previews have no lightmap. Scoped Local lighting is
an ephemeral solve, not a final export. Final Build/Build + Play save and invoke
the compiler. Build + Play exports the project for the packaged player.
[Editor workflow][editor-guide] · [Unbaked preview][unbaked] · [Quick snapshot][quick] ·
[Test lifecycle][test-runtime] · [Local solve][local-solve] · [Final build][build]

## Why a texture can make a Full build stale

An opaque wall PNG can change bounce colour through its mean albedo. Alpha
changes can affect light transmittance, and reflected captures consume albedo,
normals and emission masks. The compiler records size and SHA-256 for
referenced installed assets, including surface images/masks, fixture sheets,
decals, specified sky/appearance art and model GLBs. Those identities enter
package currentness and, conservatively, the prepared-stage fingerprint.
A visual-only celestial disc has a narrower lighting-stage exception unless
it is also used by another dependency such as a wall. Reflected sky has
separate probe provenance. The transient lightmap key alone omits resolved
PNG/GLB bytes. The compiler uses a fresh cache and verifies expected reads and
live inputs before publication. Rechecking a saved level can report a stale
package and require a new bake. Changing a declared installed PNG can make an
old package fail dependency verification on its next open even at the same
size. That is different from developer fingerprint staleness alone, and neither
case rewrites a published archive or live-refreshes an already open game.
[Transport albedo][transport] · [Alpha coverage][transport-alpha] · [Dependencies][dependencies] ·
[Lighting-stage key][lighting-key] · [Dependency digest][lighting-digest] · [Transient lightmap cache][transient] ·
[Compiler live-input checks][compile-check] · [Saved Bake check][bakes] · [Currentness report][verify] ·
[Runtime dependency checks][package]

## What to borrow from Bevy

Bevy's typed `AssetServer` keeps stable handles. Explicit `reload(path)`
asynchronously reloads live assets at the requested path. For relevant file
events, watcher dispatch separately walks reverse loader-read dependencies
and queues those paths too. Neither mechanism is the same as ordinary
handle dependency readiness. [Explicit reload][bevy-reload] ·
[Watcher dispatch][bevy-dispatch]

Successful replacement in `Assets<T>` emits `Modified`. Recursive readiness
is reported separately as `LoadedWithDependencies`, not as a GPU-ready
transaction. A failed load reports typed and untyped diagnostics and marks
load state failed without replacing the existing CPU value on that failure
path. That is not group rollback. Successful values can be inserted before
dependency readiness, reload tasks are detached, and GPU extraction can
remove the old GPU entry before a replacement is ready. Bevy can reuse a
descriptor-compatible image allocation. Do not copy that shortcut into an
experiment that requires the last-known-good image to remain untouched.
[Dependency state and publication][bevy-deps] ·
[Failures][bevy-fail] · [Events][bevy-events] · [GPU preparation][bevy-gpu] ·
[Same-descriptor GPU image][bevy-image]

The shipped desktop example uses the opt-in `file_watcher` feature. The default
filesystem source uses a 300 ms debounce and has no default filesystem watcher
for wasm or Android. The watcher recognizes rename and remove events, but the
server's reload dispatch handles only a subset and is not a rename-to-new-ID
contract. Bevy also separates unprocessed sources from optional developer
processing and processed assets. That separation is useful to Places, but
does not implement Places' compiler, bake, package verification or rollback.
[Example][bevy-example] · [Default source][bevy-source] ·
[Watcher and event dispatch][bevy-watcher] · [Dispatch][bevy-dispatch] · [Asset modes][bevy-modes]

| Option | Correctness, integration and upkeep |
| --- | --- |
| Reopen or trigger today's reprepare | **Keep as baseline.** No new maintenance. Reprepare catches many stable edits but can publish a fallback and does a scene install. Reopen replaces authoring state. Neither refreshes the browser cache. |
| Manual editor generation | **Prototype separately.** Lowest added integration. Reuse CPU/GPU preparation for one qualified PNG, but explicitly validate the candidate and retain the complete old view until commit. Keep a full preview rebuild as the comparison, not a promise of incremental GPU refresh. |
| Debounced watcher | **Defer.** Useful only after the same manual path is safe. Medium ongoing cost for missed, duplicate, coalesced and out-of-order events, replacement saves, delete/recreate, retries, reconciliation and teardown. Events are hints, not proof a writer is finished. |
| Wider editor asset service | **Defer.** High integration and maintenance across catalog versions, reverse dependencies, derived geometry, all consumers, thumbnails, budgets and frame-boundary publication. Justified only by several proven uses. |
| Integrate Bevy assets or renderer | **Reject for this scope.** Even the asset crate couples to Bevy App/ECS/tasks/reflection. Places would still own dependency discovery and its coherent renderer, physics and package transactions. No migration. [Bevy crate manifest][bevy-manifest] |

## Gate for a separately approved experiment

Use one ordinary Inspect-mode wall albedo, for example
`home:tex_wall_paint_offwhite_01` (1024 × 1024 RGB), referenced by two visible
surfaces in a small editor-authored scene. Keep its ID, path, dimensions,
opacity, tiling and colour interpretation. Use an isolated development asset
root, never the installed or published bundle. The manual command must not
save the document, change the camera or selection, consume undo, or run
scripts. Leave the committed picker tiles alone. No GLB, rig, physics,
scripts, automatic watcher, Local/Full bake or production package mutation.
[Catalog material][wall] · [Catalog PNG][wall-png]

1. Capture one bounded immutable byte buffer per candidate and use that
   revision for every binding and its derived mips. Reject unreadable,
   partial, corrupt or contract-changing bytes instead of accepting the
   diagnostic fallback. Stage beside the resident scene. Recheck content
   and current scene/request identity just before publication, then commit
   ready bindings and corresponding view state together before rendering.
   Keep committed picking state, including `play_mesh`, usable while staging
   and after a rejection. The current request clears that mesh, and
   `editing_scene_ready()` requires both a mesh and no viewport error. Report
   the rejected asset and reason through a nonblocking candidate status,
   without setting the viewport request error or marking the resident scene
   unready. Keep the last committed valid view if the latest request is
   invalid. Never publish an obsolete success. A recheck narrows the race
   but cannot prevent a subsequent write. Treat output currency as unknown
   until the normal Bake check. [Current request][pinned-view-request] ·
   [Viewport errors and readiness][pinned-view-readiness]
2. Test no-op refresh, rapid valid A/B requests, valid then truncated/corrupt,
   and subsequent valid recovery. Include in-place and temp-file rename
   saves, a locked or missing file, and scene switch/close during work.
   Verify both surfaces change on the same frame, no fallback or mixed
   revision flashes, and old camera, picking, selection, dirty state and
   undo/redo remain usable on rejection. Cap overlapping decoded and GPU
   resources, pending work and submissions. Cancel stale work and retain
   submitted resources only as long as GPU ownership needs them. Verify
   memory returns to a stable baseline over repeated success/failure cycles.
3. Compare with today's same-editor reopen and ordinary reprepare on native
   Mac and Windows. Record exact commit/binary, hardware, OS/filesystem,
   backend, settings, drawable size, scene/asset digest, warmup, samples
   and procedure. Measure from save and from command to the first correct
   present, plus peak and settled CPU/GPU memory with the same
   scene and settings. Separate warm and cold trials. No FPS benefit is
   claimed. Linux remains unverified unless separately exercised.

**Expansion is a separate decision.** Short-lived file reads must tolerate
Windows sharing violations, including delete/rename sharing, and replacement
can leave an already open handle reading old bytes. Observe both write styles
and report bounded retry or a visible rejection without switching path or
alias. Keep paths as native `PathBuf` values, confine them to the authorized
root, and test non-ASCII root/path components, case and Unicode-equivalent
names where the volume permits. Don't change Places' ASCII logical-ID
contract. A future watcher should reconcile coalesced/dropped events, not
extend itself into production. For multi-file assets, missing or renamed
dependencies must reject the candidate unless a new binding is explicitly
validated. GLBs embed their PNGs here, but model changes can still change
bounds, picking, colliders/navigation, sockets, animation bindings and shadow
topology. Retire those derived states together or leave all of them alone.
Tall Guest's cavity mapping is tied to a reviewed export, not just vertex
count. Materials/masks and fixture/decal/sky inputs need their own closure
and lighting invalidation audit. Do not promote any of those classes based
on a one-PNG result.
[Win32 file sharing][windows] · [Apple FSEvents][mac] · [Logical IDs][ids] ·
[GLB image contract][glb] · [Reviewed cavity binding][cavity]

[issue]: https://github.com/csd113/Places/issues/46
[places]: https://github.com/csd113/Places/commit/78bbbf21444f1caf710ac403deb176387c3add6a
[release]: https://github.com/bevyengine/bevy/releases/tag/v0.20.1
[bevy]: https://github.com/bevyengine/bevy/commit/a0577b5c6a7b208719dd761c5c6a7a539e472a97
[editor]: ../../src/editor/mod.rs#L159-L283
[actions]: ../../src/editor/actions.rs#L336-L395
[history]: ../../src/editor/actions.rs#L811-L831
[doc-history]: ../../src/editor/document.rs#L522-L545
[layers]: ../../src/editor/focus.rs#L189-L228
[toggle]: ../../src/editor/gameplay_actions.rs#L151-L155
[local]: ../../src/editor/lighting.rs#L81-L110
[view-request]: ../../src/editor/viewport.rs#L225-L290
[pinned-view-request]: https://github.com/csd113/Places/blob/78bbbf21444f1caf710ac403deb176387c3add6a/src/editor/viewport.rs#L233-L269
[pinned-view-readiness]: https://github.com/csd113/Places/blob/78bbbf21444f1caf710ac403deb176387c3add6a/src/editor/viewport.rs#L363-L409
[view-install]: ../../src/editor/viewport.rs#L584-L695
[worker]: ../../src/editor/viewport/preview.rs#L70-L306
[prepare]: ../../src/editor/viewport/preview.rs#L367-L470
[model-scan]: ../../src/editor/viewport/preview.rs#L473-L538
[model-cache]: ../../src/engine/props/mod.rs#L305-L432
[png-cache]: ../../src/engine/materials/image.rs#L362-L535
[reuse]: ../../src/editor/viewport/preview/reuse.rs#L120-L215
[fixtures]: ../../src/engine/materials/fixture_sheets.rs#L35-L125
[decals]: ../../src/engine/render/wgpu/decals.rs#L1019-L1072
[app-start]: ../../src/editor/mod.rs#L551-L557
[browser]: ../../src/editor/browser.rs#L65-L185
[ui]: ../../src/engine/render/wgpu/ui.rs#L219-L247
[asset-reads]: ../../src/engine/assets/mod.rs#L56-L85
[verified-reads]: ../../src/engine/assets/reads.rs#L89-L133
[gpu-stage]: ../../src/engine/render/wgpu/renderer.rs#L3904-L4089
[gpu-fences]: ../../src/engine/render/wgpu/renderer.rs#L376-L405
[gpu-commit]: ../../src/engine/render/wgpu/renderer.rs#L4641-L4692
[gpu-textures]: ../../src/engine/render/wgpu/texture.rs#L1279-L1316
[material-fallback]: ../../src/engine/materials/resolve.rs#L1043-L1162
[prop-fallback]: ../../src/engine/render/common/level_build.rs#L572-L606
[pick]: ../../src/editor/viewport.rs#L475-L508
[view-publish]: ../../src/editor/viewport.rs#L753-L795
[editor-guide]: ../EDITOR.md#quick-test
[unbaked]: ../../src/engine/render/common/level_build.rs#L629-L639
[quick]: ../../src/editor/play_here.rs#L67-L97
[test-runtime]: ../../src/editor/viewport/test_runtime.rs#L165-L235
[local-solve]: ../../src/editor/viewport/preview/progress.rs#L76-L143
[build]: ../../src/editor/actions.rs#L1114-L1133
[transport]: ../../src/engine/render/common/light_transport.rs#L746-L765
[transport-alpha]: ../../src/engine/render/common/light_transport.rs#L317-L345
[dependencies]: ../../src/map_compiler/mod.rs#L2006-L2107
[lighting-key]: ../../src/map_compiler/mod.rs#L2393-L2428
[lighting-digest]: ../../src/map_compiler/mod.rs#L2558-L2576
[transient]: ../../src/engine/render/common/level_build.rs#L1014-L1033
[compile-check]: ../../src/map_compiler/asset_inputs.rs#L133-L170
[bakes]: ../../src/editor/bakes.rs#L80-L105
[verify]: ../../src/map_compiler/verification.rs#L216-L251
[package]: ../../src/engine/package/world.rs#L489-L515
[bevy-reload]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/server/mod.rs#L775-L824
[bevy-deps]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/server/info.rs#L445-L579
[bevy-fail]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/server/mod.rs#L1890-L1942
[bevy-events]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/event.rs#L46-L60
[bevy-gpu]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_render/src/render_asset.rs#L393-L479
[bevy-image]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_render/src/texture/gpu_image.rs#L63-L107
[bevy-example]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/examples/asset/hot_asset_reloading.rs
[bevy-source]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/io/source.rs#L300-L318
[bevy-watcher]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/io/file/file_watcher.rs#L122-L248
[bevy-dispatch]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/server/mod.rs#L1950-L2047
[bevy-modes]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/src/lib.rs#L292-L315
[bevy-manifest]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_asset/Cargo.toml#L13-L40
[wall]: ../../assets/catalog.json#L1919-L1930
[wall-png]: ../../assets/catalog.json#L1748-L1756
[windows]: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
[mac]: https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html
[ids]: ../../src/engine/assets/mod.rs#L332-L344
[glb]: ../../src/engine/gltf/mesh/material.rs#L263-L277
[cavity]: ../../src/engine/lighting/character_cavity.rs#L6-L25
