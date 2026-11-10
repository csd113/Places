# Changelog

Major features, significant fixes and performance improvements by version. Minor
fixes and detailed stage accounting are omitted from the summaries. Published
prereleases use their GitHub publication dates; older milestones use source dates
where publication cannot be verified. Performance findings retain their recorded
hardware/workload scope.

The [version audit](docs/reports/changelog-version-audit-2026-11-04.md) explains
the numbering resets, tag coverage and evidence gaps. The
[former detailed changelog](https://github.com/csd113/Places/blob/3fee560e484d5dbc61496b34c90cdef8292dbe2b/CHANGELOG.md)
and linked reports retain historical implementation and validation details.

## 0.14.0 — In development

- Add a map-granted crowbar, separate from carried props, with a compact
  first-person mannequin hand/forearm. `1` holsters/redraws it; one click starts
  one swing. Attacking while carrying safely drops the prop and swings on the
  same input. Tool contact launches light props with throw-comparable force and
  applies configured HP damage only on maps that enable health. Expose damage,
  reach and swing speed through normal editor map settings. Old walking maps
  grant no tool and keep health disabled.

- Add a local player mannequin with eight saved colour choices, defaulting to
  muted teal (`#4F7774`). Keep the body hidden in first person, including when
  looking down. On enabled-health maps, death transfers the mannequin to the
  existing ragdoll solver and keeps vision and audio attached to its physical
  head. Explicit respawn, map reload and editor test exit retire the old body.
  Held-tool arms and multiplayer spectating remain future work.

- Add an opt-in player/entity health foundation, disabled by default for new
  and existing maps. Enabled maps use six HP (three hearts), half-heart damage
  and healing, typed death changes, and checkpoint persistence.
  Map authors can set starting and maximum health separately for the player
  and individual NPCs, and independently configure regeneration, instant kit
  healing and visible hearts. Hit protection is configurable with a zero-second
  default. No screen effects, weapons or multiplayer respawn policy is added.

## 0.13.1 — 2026-11-09

- Add an illustrated beginner editor guide in seven GitHub-readable chapters
  and a downloadable PDF, with visual Simple-mode construction, native
  screenshots, an editable example and a verified fresh-project replay through
  build, export and play.

- Add The Long Way Home, a three-part journey with a Feltling guide, Tall Guest
  pursuit, two-storey passages and crouch refuges, physical distractions and a
  duck-carrying homecoming. Preserve its complete editable project in the release.
- Expose native Feltling and Tall Guest authoring in the editor. Improve grounded
  creature traversal, live obstacle replanning and player clearance; correct
  ceiling-emitter projection and retain contact detail in baked lighting.
  Further corner-light review is tracked for 0.14.0 in issue #49.

- Add original single and paired flush hollow-metal exit doors with
  surface-mounted vertical-rod panic hardware, editable Blender/GIMP sources
  and interactive editor presets. Share door actions, locks and Luau state
  queries with rigid clip mechanisms; keep collision and appearance on one
  accepted pose, and ground separate frame/leaf contacts rather than an empty
  aperture. No automatic closing, certification claim or asset-cap increase.

- Add reusable live in-engine cinematics through ordinary trigger/sequence
  events, out-of-body move/follow/orbit/look-at shots and existing Luau NPC
  actions. Add a small Advanced authoring sheet, exact-key completion/level
  transitions, scoped input ownership and safe cancellation/queue-full recovery.

- Add three original, opt-in tonal ambient loops: Threshold Air, Pale Signal
  and Below the Landing. Expose them through the normal sound picker without
  changing authored map selections. Preserve the original Empty Room master;
  explicit narrow-band runtime formats keep all cues inside the unchanged
  decoded-audio cap, with reproducible per-file import and audit settings.

- Add opt-in Tall Guest acoustic pursuit and mouth capture of a full-size
  Feltling, followed by an articulated lean-back swallow and recovery. Keep one
  physical/render owner, exact-generation retirement and existing asset/audio
  caps. Preserve legacy animation, reconstruct deforming mouth normals and
  regenerate mouth-only cavity shading for the extended rig.

- Simplify compatible touching wall sections and continuous recessed-floor edges
  in Full-lighting preparation without merging their editable sources. Preserve
  openings, material boundaries and Static lighting detail. Keep tall recessed
  edges' shading continuous when they span multiple lightmap sections.

- Avoid unused translucent-character batch copies and lighting uploads when no
  main, mirror or probe view consumes them. Skip UV receiver sampling while all
  UV sources are off, and defer fully faded actors' receiving-light refreshes
  until they reappear. Preserve first-return lighting and attached lights.

- Correct illumination from flashlights and other attached lights on dark,
  pre-lit surfaces. Keep surface marks and details behind transparent objects and
  blend them into fog properly. Show the sky and celestial objects in mirrors and
  apply weather effects more consistently to reflected characters. Make thin
  glowing details less likely to disappear from the glow effect. Preserve
  individual prop colour tints and texture edges when generating smaller texture
  versions.

- Avoid rebuilding unrelated objects' resources when characters or props are
  added or removed, helping avoid scene-change hitches. Reduce repeated work for
  transparent objects, reflections and views that are not being displayed.
  Prepare flashlight effects earlier to reduce first-use hitches, and move some
  sky preparation and graphics setup away from moments that could interrupt
  gameplay. Skip objects that cannot appear in a reflection and characters
  outside an auxiliary view earlier.

- Improve recovery from display interruptions without presenting an untouched
  frame during normal recovery. Detect oversized lighting data before graphics
  allocation fails, fix handling of certain texture sizes and reduce unnecessary
  temporary allocations.

Matched M2 Pro/Metal tests at 1920×1080, High/Full, reflections off and glow on
show nearly unchanged average FPS in the close-up and doorway scenes, with
substantially improved worst close-up frame times. A six-ghost scene is about
1.8% slower; a focused control points to the corrected glow effect as the added
cost. These scene-specific results do not establish a general FPS gain.

- Integrate the benevolent Feltling guide with authored routes, wait/resume and
  strictly-over-20-metre attention gestures, reusable conversation/celebration
  hooks, native animation ownership, checkpoints and original spatial babble.

- Keep held-prop pushes consistent across control rates and improve heavy/light
  resting contacts while guarding supported jams. Correct the native contact
  fixture. See [Mac validation and remaining costs](docs/reports/prop-physics-0.13.1-mac.md).

- Archive five retired projects with all editable sources, scripts and thumbnails.
  Keep Beam, Backrooms and Quiet Collection active; refuse archived identities in
  new desktop exports and retain useful development controls.

- Protect hidden and isolated attachments during numeric/exact transforms and
  room-edge edits; move explicit spawn heights and owned elevator landings/links
  with isolated rooms, refusing shared shafts with a cause. Reset map
  isolation and authoring drafts on map/project changes, and allow source recovery
  from an empty editor.
- Bound bake verification output, cancellation and shutdown; enforce document
  memory admission on initial Open and map loads. Preflight script creation before
  publishing files, source links or history, including generated templates.

- Count eligible and visible geometry in matching world, prop, dynamic, character,
  decal and effect scopes; keep resident storage and actual draw commands separate.
- Gate water optics on the installed water triangles and conservative opaque
  coverage. The Pool dry-gap control removes a 7,372,800-byte snapshot and the
  extra water pass with identical pixels, while retaining visible-water effects.
- Reuse ray reciprocals and prepared triangle edges for runtime contact/cabin
  visibility without reducing ray budgets or relaxing solid-world checks.

- Clip movable contact shadows to supported floor receivers and solid world geometry,
  select the nearest visible footprints within eight slots, and block open-cabin
  light behind opaque hall geometry. Preserve valid doorway illumination and
  closed seals. See [Mac validation and frame cost](docs/reports/runtime-lighting-0.13.1-mac.md).

- Remove repeated handle scans from actor collision avoidance. Keep sight alerts
  stable across perception batches, compare every shelter within the shared path
  budget, release delivered-item reservations between routine steps, and honor
  authored long waits. Elevator NPCs retain cabin-local shelters through rotation;
  departure-map shelters become the passenger's arrival post. Home regions remain
  map-bound and cannot travel.

- Protect editable compiler inputs and concurrent output edits during publication.
  Reuse verified navigation and project member streams after display metadata
  changes, reducing the measured collection refresh from 17.27s to 3.44s. Offer
  direct lossless compact build/export, saving 15.3% on the Low/Static collection.
  See [Mac evidence and tradeoffs](docs/reports/compiler-safety-storage-0.13.1-mac.md).

- Safely release a persistently obstructed held prop once it becomes too far
  away, preserving its current pose. Passive carry advisories appear only in
  developer mode. See [Mac validation](docs/reports/held-auto-drop-0.13.1-mac.md).

- Elevator cargo supports **32 distinct authored movable props/NPCs per project
  session**, including travellers left in visited levels, across rides and
  checkpoints; the player is excluded.
  Bodies must fit entirely. Runtime spawns, ragdolls/articulated or NPC-held
  assemblies, and map-bound wiring/tasks/effects are unsupported.

- Keep elevator cabins and decks stationary during sealed rides, including
  legacy vertical landings. Preserve exact passenger poses, props/NPC ownership
  and safe recovery; add a bounded in-car motor cue. Refresh the collection's
  doorway dependencies and development package versions. See
  [native travel/audio evidence and limits](docs/reports/elevator-stationary-ride-0.13.1-mac.md).

- Fit thin metal elevator doorways flush to the cabin and landing walls, close
  floor-skirt gaps, and automatically align editor placements with real wall
  faces. Fit openings in one undoable edit and reject insufficient space. See
  [art, placement and review evidence](docs/reports/elevator-doorway-fit-0.13.1-mac.md).
  Hollow thick walls and sufficiently large rectangular or circular pillars
  around the actual cabin volume; wall depth alone does not reject placement.
  See [pillar acceptance](docs/reports/elevator-pillar-fit-0.13.1-mac.md).

- Keep elevator cabin light inside the shell and real sliding-door aperture.
  Closed exterior panels use actual landing lighting instead of the roomless
  ambient fallback; the interior stays lit and hall fixtures still light the
  panels. Add explicit dark-room and lit-hall controls. See
  [Mac validation and limits](docs/reports/elevator-light-0.13.1-mac.md).

- Include automatic HVAC, room/pool/fluorescent ambience, material footsteps and
  moving-door sounds in editor quick testing through the player's shared audio
  policies. Preserve authored emitters and quiet chooser auditions; clear test
  voices on Stop, Escape and focus loss. See [Mac evidence and limits](docs/reports/release-0.13.1.md#editor-quick-play-automatic-audio-2026-11-04).

- Replace the Quiet Collection's eight hub travel doors and eight return doors
  with signed passenger elevators. Preserve favors, saved progress and the
  reward room; return to the matching hub bay. Collection package 1.0.4 adds
  fitted shafts, updated guidance and an explicit direct-binary route check.
  See [Mac acceptance and limits](docs/reports/hub-elevators-0.13.1-mac.md).

- Batch ready level-upload work within a short CPU slice while retaining GPU
  queue and resource bounds. Matched Mac Metal loads improve 30-43%, and real
  cross-level elevator loads improve 45-47%. Add opt-in archive/stage attribution
  to the loading harness; preserve exact scene output and save/recovery behavior.
  See [measurements and limits](docs/reports/release-0.13.1.md#level-loading-audit-and-bounded-upload-batching-2026-11-04).

- Simplify the editor workspace with a brighter dark slate palette, quieter
  controls, common building tools and progressive customization. Test the current
  unsaved scene in first person inside the editor window without a lightmap bake
  or separate player, then return to retained authoring state. Final builds
  preserve production lighting validation. See [workflow and limits](docs/EDITOR.md).

- Author the hub's existing wall-sign decals in both editor modes: printed
  artwork selection, fitted size/spin, solid wall placement, facing, clear-space
  copies, selection/locks/history and source/package persistence. Retain the
  committed PNG lettering and shared decal depth/lighting path. See
  [Windows evidence and limits](docs/reports/wall-signs-0.13.1-windows.md).

- Specialize opaque model shaders to remove the unreachable editor ceiling
  discard while retaining authored faces, materials, lighting and Full reflections.
  Matched Windows pairs improve Hollow and Arcade throughput; record frame tails,
  pixel comparisons and added loading cost in the
  [FPS optimization report](docs/reports/fps-optimization-0.13.1-windows.md).

- Add optional per-level gameplay checkpoints with editor volumes, shared script
  actions, compatible Continue/Start new choices and bounded atomic local saves.
  Restore authored gameplay and explicit script state through stable identities;
  retain disabled-by-default maps and clear unsupported-state refusals. See
  [the persistence contract](docs/CHECKPOINTS.md) and
  [Windows validation](docs/reports/checkpoints-0.13.1-windows.md).

- Keep loading UI/event pumping active during first-use world shader preparation.
  Preserve actual milestone progress, main-thread presentation and offline builds.
  See [Windows startup evidence and limits](docs/reports/startup-0.13.1-windows.md).

- Present ordinary NPC messages as queued two-line subtitles, retain F1 help,
  simplify loading to actual stage progress with separate unknown-work activity,
  and give Explore cards roomier spacing, contained titles and full-name details.
  See [Windows validation and remaining limits](docs/reports/ui-0.13.1-windows.md).

- Restore continuous grounded stair support across the full player hull, retain
  real tread contacts for jumps/falls, and ease the first/last riser camera
  within live clearance. Raise the shared scene clipping/culling distance from
  100 to 256 m for long Backrooms sightlines.
- Refresh water reflection probes when their actual light sources switch, keep
  unfilled directions dark, and preserve authored sky radiance. Water's fallback
  sheen uses the incident energy already folded into its vertices. Existing
  source eligibility still excludes fluorescent and moonlight caustics.
- Launch the Windows release game directly without a console window. File
  diagnostics and error notices remain; explicit CLI/verbose use can attach to
  the caller's console. Editor/compiler command-line entry points are retained.

- Add Reuse tools in both editor modes: furnished room/corridor source capture,
  repeated independent or linked stamping, explicit three-way linked updates,
  category-based room styles, and seeded prop yaw/scale/sinking variation.
  Private previews, cancellation, one-step history and ordinary source
  persistence share the existing editor path. Linked overrides/deletions survive;
  missing sources retain geometry. Optional standing-hull clearance protection
  uses real collision samples, and saved baselines prevent seed drift.
  See [the workflow and bounds](docs/EDITOR.md#reuse-tools-room-prefabs-room-styles-and-prop-variation-0131).

- Add the cloud-authored 25-model utility/HVAC kit, retained Blender/GIMP sources,
  six-axis duct joins with reversible previews, five circuit-ready fluorescent
  variants and a bounded editor showcase. Preserve the original atlas and raise
  the decoded prop pack ceiling to 128 MiB. Static doors and passive fixtures
  are labeled explicitly; existing janitor cart/supplies are reused.

HVAC hum now belongs to one nearby spatial duct voice, with squared falloff,
duplicate suppression and removal/map-transition cleanup. Native kit placement,
circuit lighting, source export/import and actual-duct audio acceptance are
recorded in [the phase-6 report](docs/reports/release-0.13.1.md#phase-6-utility-kit-duct-joins-and-local-hvac-audio).

Phase 5 adds contextual light-circuit selection/painting and one-step switch
assignment, native power that composes with existing flicker and prepared Full
illumination, and cancellable non-final room/region transport previews. Local
previews retain outside lights/occluders and invalidate on edits without writing
final exports or bake caches. Conservative CPU work reductions preserve exact
physical output on bounded matched fixtures. Measured compiler/GPU investigation,
quality evidence, native lifecycle validation and limits are recorded in
[the phase-5 report](docs/reports/release-0.13.1.md#phase-5-circuits-compiler-investigation-and-local-lighting-preview).
GPU transport remains experimental; production uses the accepted CPU solver.

First editor phase: rectangular and circular pillar creation and fitted automatic
trim, using the existing engine primitives and baseboard preparation.

- Geometry exposes both pillar shapes. Selected pillars keep the ordinary
  material browser, transforms, duplication, deletion and source history.
  Blank base/height follow the floor/ceiling; circular size has one diameter.
- Pillars offer none, bottom, top or both trim bands. Selected walls expose
  Auto trim with a clickable wall set, dimensions/material, preview and one-step
  undo. Regeneration replaces derived output after geometry changes.
- Fitted rectangular/round/curved joins share contact clipping before lightmap
  creation, continuous trim texture distance and visible ceiling-band undersides.
  Unsupported floor/roof contacts produce named feedback. Deliberate angles and
  authored positions are retained; published maps/assets remain unchanged.
- Floor/ceiling charts exclude buried receivers at contacts, correcting the
  reproduced false-gap dark seam at bare and trimmed walls without suppressing
  lighting or shadows. Clipped triangular receiver edges choose their owning
  air side; microscopic clipping residue is removed before publication.

Workflow and fit limits: [editor guide](docs/EDITOR.md#pillars-and-automatic-trim-0131).
This is an unreleased bounded phase; later queued editor work remains separate.

Phase acceptance on the Razer (Windows, RTX 3070 / Dx12, 2026-10-28):
format, strict workspace/all-target/all-feature Clippy and locked release build
pass; 365 Rust tests and 369 source tests pass (18 expected source-test skips).
The asset validator checks 644 assets with zero warnings. Five small CPU
regressions cover regeneration, source/history, joins, receiver ownership and
real shadow preservation; ordinary tests do not bake production maps.

Native evidence includes 326 editor verification actions, all pillar trim modes,
material browser/picking, preview/source isolation, copy/delete, undo/redo,
repeat wall-set Apply, Save/Build, exact export/source recovery and final editor
lighting. Twenty final compiled-game views cover rectangular, circular and
angled curved junctions, floor/ceiling bands, wood corner alignment, bare-wall
comparison, albedo/normals/lightmap diagnostics and both pillar collisions.
The small fixture has zero geometry errors/warnings; unsupported open-ceiling
and raised-floor contacts produce named refusals. The fixture and authored
project export pass integrity and `verify --require-current` checks.

Razer evidence and hash-verified executables are retained outside `target` under
`local/verification-0.13.1-trim/`: `validation-receipt.json`, `accepted-bin/`,
source projects, packages, recipes and framebuffer captures. These are phase
checks: production 0.13.0 packages/assets are untouched; full 0.13.1 production
rebake and CI/release closing remain later. Fifo was negotiated, so these runs
make no uncapped FPS claim.

Second editor phase: Simple mode now exposes New Room, Add Wall, Add Door,
Add Light and Play. Make Doorway explicitly creates an opening; Add Door creates
or fits an interactive leaf. Click-start/preview/click-end wall tools support
chaining, grid/endpoint snapping and cancellation. Wall-body dragging and mint
endpoint handles preserve openings, doors, wall fittings and automatic trim with
atomic undo. Room stretching retains its attachment and shared-boundary guards.

Layers/focus supplies four visibility groups, isolation and saved object locks.
Material browsing defaults to 24 smaller labelled tiles in four rows, with an
adjustable size, search, paging and selected state. Responsive control/hit mapping
uses logical window size, display scale and drawable density; Simple Play saves,
exports, builds and launches automatically. Eleven missing offline material
previews now use the existing committed PNG pipeline.

Phase acceptance: strict Clippy, locked release workspace build, 367 Rust tests,
369 source tests (18 expected skips), and the 644-asset validator pass. Native
Windows acceptance covers creation, chained walls, opening/leaf distinction,
materials, locks/layers, attached wall edits, room stretch, save/reopen and
Play/return, including opening and walking through the new door. Actual Razer
layout checks use 150% display scale at 960x544 through 1440x900; CPU coordinate
checks also cover 100/125/200% and doubled drawable density. Actual Mac Retina
acceptance remains deferred. See [current acceptance and limits](docs/reports/release-0.13.1.md).

Third editor phase integrates construction integrity into the existing Safety
review, now directly available in both modes. Shared checks cover short corner/
endpoint gaps and tails, wall planes/thickness, floor/ceiling seams, openings,
sampled route/landing/headroom and measured material UV phase discontinuities.
Previewed compatible corrections form one undoable group; stale, conflicting,
locked/hidden or protected edits refuse atomically. Existing wall attachments,
offline alignment planning, mounting, collision and geometry union/clipping are
reused. Cancellation discards partial results. Angled geometry, artistic
intersections and unsupported UV/material corrections retain explicit review.
See the [editor workflow](docs/EDITOR.md) and [phase acceptance](docs/reports/release-0.13.1.md).

Fourth editor phase adds backed wall recesses/TV niches, fitted door/window
surrounds and sills, railing/fence paths with shared posts and stair following,
and point-to-point crouch tunnels. All use private preview, atomic history,
save/reopen and derived regeneration. Source admission and shared Safety reject
unsupported cuts, routes, headroom and corner fit. Frames clip existing wall trim;
rail corners meet at miters. Simple mode exposes the tools through geometry/Build
controls while retaining the wall/door/light tools and 24-tile material browser.
See the [supported routes and controls](docs/EDITOR.md#construction-generators-0131)
and [phase acceptance](docs/reports/release-0.13.1.md).

### Completed audit and repair coverage (4–5 November 2026)

Reuse the linked fixes and regression evidence before repeating broad audits of
unchanged code. This dated record covers the scopes below; later changes, new
evidence and regressions still need targeted review.

**Astra Extra High audits followed by Sol 6.1 Extra High repairs:**

- **Map compiler:** Source/output publication safety, metadata-only stream reuse
  and compact export. [Fixes](https://github.com/csd113/Places/commit/52f03b0a12c9eed2f4d58d950ce4169c8d6a50a6).
- **AI code:** Actor avoidance/perception, shelter/path budgets, reservations,
  authored waits and transported shelter ownership.
  [Fixes](https://github.com/csd113/Places/commit/fc0852944620803fc6023e04f536d6115131d6cf).
- **Runtime lighting:** Contact-shadow receivers/selection and elevator light
  occlusion. [Fixes](https://github.com/csd113/Places/commit/90aaffa7ebd92fa6d2335ea16cff2764a1bb0de2).
- **Full level editor:** Attachment transforms, room-owned coordinates,
  focus/recovery, bake shutdown, memory admission and script publication.
  [Fixes](https://github.com/csd113/Places/commit/68136b44f14f3abca83a0699e18f3e9032f9672f).
- **Level transitions, saves and recovery:** Bundled menu, failed campaign writes,
  NPC navigation pace and remaining light-release holds.
  [Fixes](https://github.com/csd113/Places/commit/806944a477516405d0c0172cc834762f7fb8ada3).
- **Scripts, triggers and progression:** Pool hint order, Beam travel retries,
  interaction reset and timer repeat defaults.
  [Fixes](https://github.com/csd113/Places/commit/2fb2691e7a4be4378db9241abff774d59a9d04c0).
- **Player movement and collision:** Hull corner stepping, shared footstep hearing
  and the separately requested crouch/walk sound bounds.
  [Fixes](https://github.com/csd113/Places/commit/d9108749edbe4f53a77af35268e595ef0956cd98).
- **Asset loading and resource lifetime:** Entity-handle reuse, bounded editor
  child/output-reader cancellation and the corrected audio-lifetime assumption.
  [Fixes](https://github.com/csd113/Places/commit/900a468766860ede5a060f0452e5a8abf78f2777).
- **Audio and acoustics:** Floor/ceiling/elevator barriers, initial spatial
  occlusion and storey/face reverb overlap.
  [Fixes](https://github.com/csd113/Places/commit/81788355ec5e898a7188ed1d1857c09b431c5f23).
- **Runtime map/package loading:** Concurrent import protection and
  semantic/nested allocation admission.
  [Fixes](https://github.com/csd113/Places/commit/9c05acf8d242e49274bb6a12412cefb3846980e7).

**Completed original scopes with a later repair-model change:**

Astra audited these scopes; Sol began the repairs and Astra Extra High completed
them after an explicit model change.

- **Rendering performance, culling telemetry and water:** Matching geometry
  counters, hidden-water work and runtime visibility-ray preparation.
  [Fixes](https://github.com/csd113/Places/commit/124f6c85003a74d6cfefbd123f3ffa3db0a6ab11).
  This predates the later character self-lighting work.
- **Physics props and interactions:** Control-rate-independent held pushes,
  resting contacts, supported jams and the native contact fixture.
  [Fixes](https://github.com/csd113/Places/commit/bd0044eb984a6150ee0b26b8c8562ac4dfbb2f4d).
  The recorded stress/settling costs remain.

### Completed wgpu reviews (6–7 November 2026)

Astra Extra High performed both the migration/correctness and deeper foundational
performance audits and their renderer repairs, including the final foundational
follow-up. Coverage includes GPU-resource ownership/reuse, repeated view work,
lighting/transparency/reflections, frame recovery, loading preparation, texture
processing and glow footprints. See the
[implementation through 2c47601b](https://github.com/csd113/Places/compare/7074be0c61601677e2d40eb38ddea54586b32118...2c47601b78e16938ec301d5cf584d1d0f4c9f8f2)
and the player-facing visual, smoothness and measured-performance entries above.

This record ends at `2c47601b`. Later Tall Guest/Feltling character self-lighting
and mouth/floor lighting/visibility work is outside this completed-review record.


### Rust test-value review (7 November 2026)

Reviewed all 678 repository-owned Rust files and 528 test definitions at
`624e4da`, then reconciled every one of the 30 changed Rust files through
`228c24a`, including the new Flee-policy regression. Retain all **529 tests**:
small fixtures protect distinct behavior, validation, compatibility, resource,
geometry/lighting, lifecycle, input/physics/AI, script and platform contracts.
Shared names or setup alone did not justify a whole-test deletion or merge.

Remove four nonessential assertion sites: a by-value local-copy tautology, two
toy alpha-compositor checks after exact production order is asserted, and a
package fixture self-check that did not inspect output. Keep actual ordering,
copy-predicate, byte-exact recovery and overwrite-refusal guards; clarify test
names. Replace 32 identical immutable form-fit queries with one while keeping
the pose comparison and both movement sequences. Move the unchanged three-case
Feltling planner regression into its owning tests folder. Production behavior,
assets and non-Rust/native acceptance fixtures are unchanged. No suite-speedup
claim or test-count target is inferred.

Full per-test, helper and case inventories are preserved in the private
`Places_Rust_test_value_review_624e4da_2026-11-07.zip` and its current-source
reconciliation receipts. This dated assessment avoids repeating an unchanged
full audit; later changes and new evidence still need targeted review. Native
capture/swallow/obstruction proof remains separate from ordinary Rust tests.
Fresh Mac gates pass: 529 Rust tests, strict debug/release Clippy, locked
all-feature workspace builds and 11 production Luau refusals. Six Unix-only
tests account for the expected Windows source count of 523; Windows execution
is a separate pending gate. No new native or FPS acceptance is claimed.

## 0.13.0 — 2026-10-28

- Ship seven playable projects in the Windows portable distribution: **The
  Backrooms**, **The Quiet Collection**, **It Only Exists in the Beam** (four
  chapters), **Closing Time**, **One Guest Too Many**, **The Shape of an Exit**
  and **After Hours — Rooms Beyond**. Include the player, native editor,
  compiler, runtime dependencies and editable prepared project archives.
- Give each Quiet Collection destination a physical favor, remember completed
  favors across sessions and unlock the hub reward room. Keep campaign progress
  separate from general gameplay checkpoints, which arrive in 0.13.1.
- Add shared AI behaviors and native authoring for fetching, carrying and other
  tasks, perception, interruption and script events. Add safe prop-shaped player
  forms, mannequin appearances, articulated ragdolls, miniature bowling and
  bounded blood effects.
- Replace movement with fixed-step acceleration, friction, air control, jumping,
  crouching and shared collision. Repair airborne ramp fall-through, dormant
  prop activation, unwanted player-driven NPC animation, lost quest-item
  recovery and incoming contact at doors. Retain the reported stair cases that
  could not be reproduced as limitations in the release report.
- Improve buoyancy, currents, submerged carrying and physical NPC item handoff;
  preserve the actual item through attachment, release and recovery.
- Add an always-bundled live main menu, Explore/pause/style controls and readable
  bounded notices. Add visual editor browsers, per-level build planning,
  structural floors/ramps, solid generated stairs and improved bake progress.
- Add circuit flicker authoring, source-aligned Sun/Moon imagery and corrected
  lighting invalidation. Extend spatial audio with HRTF, occlusion, material
  acoustics, quieter ambience and persistent master volume; remove duplicate
  per-frame audio-mix publication.
- Reduce map storage and compiler work through compact lightmap records,
  bounded compression, dependency-aware reuse, per-level refreshes and indexed
  Static visibility. Reuse solved lighting when only unrelated or visual-only
  content changes.
- Reduce runtime CPU/GPU work and retained memory through shared immutable
  geometry, automatic instancing, eligible GPU skinning, exact pose reuse,
  cached lighting/visibility, shared AI scratch memory, bounded texture uploads
  and explicit cancellation/unload ownership. Improve incremental editor preview
  and resource retirement without lowering authored quality.

[Release notes](https://github.com/csd113/Places/releases/tag/v0.13.0) ·
[Implementation, measured results and limits](docs/reports/release-0.13.0.md).
Native acceptance covers Windows/DX12 and macOS/Metal. Improvements are workload
specific; accepted water-rendering costs, cold input stalls and uncapped Mac
presentation uncertainty remain documented. Linux/clean-OS installer acceptance
and physical audio hotplug coverage remain open. Relay Annex stays source-only.

## 0.12.0 — 2026-10-19

- Introduce the native **places-editor** with the shared game renderer,
  selection/transforms/snapping, asset browsers, undo/redo, project create/open,
  conflict-aware atomic saves and protected Build + Play/cancellation.
- Add editable multi-map projects with stable identities, explicit entry points,
  shared transitions, thumbnails and lazy level loading. Export exact JSON,
  metadata, Luau and prepared lighting together; recover a project atomically
  into a new folder without overwriting existing work.
- Add sandboxed project-local Luau, typed game actions, bounded execution and
  memory, native script editing, diagnostics and trigger-handler authoring.
- Connect Home, Pool, Office, Arcade, Lantern Hollow, Winter, Beach and Frutiger
  Aero through the central hub as the nine-map **Quiet Collection**. Expand
  Home's caretaker handoff quest, Pool's second basin/janitor closet and Office's
  multi-floor access.
- Add the separate **Closing Time**, **It Only Exists in the Beam** and
  **Relay Annex** campaigns using ordinary project and scripting features.
- Add editor ramps, raised/recessed regions, upper floors, floor/ceiling
  openings and locatable safety review. Correct covered below-floor ramps and
  redundant ceiling backing that blocked upper-storey item carrying.
- Add layered navigation for stacked floors, body-clearance links, live-obstacle
  avoidance and bounded retries; expose route/body inspection in the editor.
  Bound preview resources and use the selected map's manifest on graphics-cache
  hits so quality changes work beyond the entry map.
- Ship a self-contained, ad-hoc-signed **macOS arm64** app with sibling
  editor/compiler, relocated SDL3, launchers and all four project packages.
  Add executable-relative resources, platform user folders, validated imports,
  native startup errors and clearer import-folder discovery.

[Release notes](https://github.com/csd113/Places/releases/tag/v0.12.0) ·
[Evidence and known defects](docs/reports/release-0.12.0.md).
This prerelease has no general gameplay save/load. Reported Home/Pool slowdowns,
prop contact, Relay Annex item loss/animation/terrain and ceiling z-fighting
problems remain open here; do not read the later 0.13.0 fixes into this release.
Windows/Linux binary acceptance was not completed for 0.12.0.

## 0.11.0 — 2026-10-17

- Separate reusable engine, game and Map Compiler responsibilities and update to
  Rust 1.99.1 with reviewed dependencies.
- Introduce bounded Kira/SDL3 audio: spatial emitters, material footsteps,
  switches, doors, prop impacts, environment cues, 24 licensed cues/63 WAVs and
  persisted volume/mute. Add filtered, swept item/player triggers.
- Extend the console with curated pickups, safe reset/removal, trigger
  inspection, stopped world time/fixed stepping, model representation and
  third-person/session-local copies.
- Repair renderer surface/capture lifetimes, replacement/reload ownership and
  failure handling. Cache visibility, changed-only emission and pass selection
  to avoid repeated submission work; add bounded overlap diagnostics and repair
  the verified Office desk overlap.
- Correct small-scale normal transforms, emission-mask identity, display-copy
  shaders and the close-up flashlight source position while retaining shadows.
  Seal fixture housings and confine lamp emission to their luminous faces.
- Make water caustics source-aware, independently colored and visibility-tested,
  with authored tint and gentle motion; exclude ineligible sources.
- Harden archive/model/image bounds, canonical package identity and transactional
  imports. Share decoding/cache allocations and preserve installed maps on failure.
- Repair partial settings/custom bindings and focus/pause transitions. Correct
  entity sequence cancellation, stopped-time ownership, live-door navigation,
  animation handoffs and route completion; bound retries/perception work and
  avoid repeated skinning of unchanged poses.

[Release notes](https://github.com/csd113/Places/releases/tag/0.11.0) ·
[Validation and scene-specific performance](docs/reports/release-0.11.0.md).
Source prerelease, with macOS/Metal native acceptance; broad Windows/Linux
acceptance and physical audio hotplug testing remain outstanding.

## 0.10.0 — 2026-10-16

- Add reusable rigid-item physics, carrying/throwing, bowling resets and
  volume-aware water behavior, with a precise outline interaction hand.
- Add an occluded handheld flashlight, synchronized Office fixture response,
  source-lit Pool caustics, linked railings/fences and restrained bowling-lane
  reflections.
- Integrate completed Arcade, Shared, entity, Pool, Office, Winter and Outdoors
  model deliveries with file-backed textures and measured placement contracts.
- Reduce redundant lighting calculations and atlas uploads, parallelize bounded
  runtime preparation and reduce measured Arcade frame costs.
- Add the opt-in developer console and native **places-compile gui** with file
  pickers, Static/Full builds, real per-pass progress and cooperative cancellation.

[Release notes](https://github.com/csd113/Places/releases/tag/0.10.0) ·
[Validation, performance and limits](docs/reports/release-0.10.0.md).
Source prerelease; native acceptance is macOS/Metal. The later refined Spooner-Man
delivery is unconfirmed and the separate rubber-duck rebuild is excluded.

## 0.9.0 — 2026-10-13

- Expand the playable lineup to eight environments: Pool, Home, Lantern Hollow,
  Winter, Beach, Frutiger Aero, Cosmic Lanes & Arcade and Office: After Hours.
  Add Arcade's birthday room, ultraviolet materials and connected restroom;
  connect Office's three floors and basement encounters.
- Integrate linear HDR color/stylized materials, improved static transport,
  spatial entity lighting and corrected model lighting. Redesign menus around
  shared layouts and mouse interaction.
- Make builds, ordinary tests, CI, default exports and startup independent of
  installed maps; move Demo/Zoo to historical sources and keep packages external.
- Separate Static/Full lighting from texture quality, compact map storage,
  reuse lightmap packing holes and GPU capture resources, and add compiler
  terminal progress.
- Repair Office pressed-wall camera penetration/jump escape, terrain contacts,
  recovery, rail mounts and lighting. Correct animated-entity culling and bounds.
- Harden drawable/window ownership and retain bounded crash diagnostics with
  readable failure notices. The original intermittent crashes remain unattributed.

[Preparation commit](https://github.com/csd113/Places/commit/5b9340381c74) ·
[Final preparation report](docs/reports/release-0.9.0.md).
Versioned and validated in source; no 0.9.0 tag or GitHub Release is present.
Native acceptance is macOS/Metal.

## 0.8.0 — 2026-10-04

- Replace desktop OpenGL/GLES2 with native **wgpu/WGSL** rendering and **SDL3**
  windowing/input: Metal on macOS, Vulkan on Linux and Direct3D 12 on Windows.
- Add offline **.placesmap** compilation and a player that loads prepared
  geometry, collision, navigation, HDR/directional lightmaps, irradiance probes
  and reflections without runtime baking. Parallelize bounded bake workers
  while retaining exact-output checks.
- Add real falls, jumping, crouching, ladders, prop-top support, swimming and
  continuous water entry/exit. Repair doorway teleports, player sweeps,
  vertical contact and stale ground support.
- Add stable instance identities, object interactions, area triggers, entity
  routes/poses, reusable components, sequences and spawning. Add doors, light
  switching and steam/effects through the common action system.
- Add curved geometry, a larger Model Zoo, outdoor/night assets and sky,
  Halloween characters, the Demo night route and **Lantern Hollow**. Retire the
  legacy browser editor while preserving authored map sources.
- Repair water/translucent light transmission, lighting energy/continuity,
  directional entity lighting, wall/doorway alignment, cottage seams and asset
  geometry. Preserve authored normals and static model lightmap receivers.
- Enforce Rust 1.99 lint/CI gates and harden numeric/index bounds, resource/FFI
  ownership, atomic package publication, worker failures and CLI validation.

[Release notes](https://github.com/csd113/Places/releases/tag/0.8.0) ·
[Snapshot changelog](https://github.com/csd113/Places/blob/0.8.0/CHANGELOG.md).
Source prerelease; native validation covered macOS/Metal. Linux/Vulkan and
Windows/DX12 were target backends, not validated platforms in this campaign.

## 0.7.0 — 2026-09-24

- Preserve the final **OpenGL/GLES2 baseline** before desktop wgpu migration,
  with Full/Low Places Demo visual references and a documented benchmark route.
- Consolidate the work since 0.6.0: generic point/rectangle/line lights,
  independent true emission, broader multi-material GLB import, Full/Low quality
  profiles, baked lightmaps with static-prop occlusion and a dynamic-object path.
- Add surface response, transparency/glass, offscreen presentation,
  post-processing and selective reflections; expand the Demo's material and
  furnished Home/architectural assets.
- Add persistent Graphics/Display/Controls settings, immediate graphics/display
  changes, rebindable controls and correct Retina drawable sizing.
- Repair partition/vertical lighting isolation, coplanar surface artifacts,
  decal depth ownership and doorway-floor z-fighting.

[Release notes](https://github.com/csd113/Places/releases/tag/0.7.0) ·
[Baseline snapshot](https://github.com/csd113/Places/blob/0.7.0/CHANGELOG.md).
The baseline commit is dated September 23; GitHub publication was September 24.
The tag packages existing work and does not include the later SDL3/wgpu,
swimming or interaction implementations.

## 0.6.0 — 2026-09-22

- Introduce the official **Places Demo**, connecting Office reception/workrooms,
  an elevated window, stairs, recessed Pool and a final unfinished corridor.
- Add standalone flat/macOS app packaging, executable-relative asset discovery,
  actionable missing-resource errors, Places branding and installation guidance.
- Make the Demo the sole bundled level while retaining developer fixtures and
  the complete asset catalog for custom maps; externalize the arrow/stripe decals.
- Correct recessed-floor transition faces using the wrong wall material.

## 0.5.3 — documented lighting milestone

- Stop fixture light and color crossing opaque walls while preserving real
  door/window transmission; light wall faces, corners and reveals from their
  correct sides and limit doorway blending to the visible aperture.
- Mirror the visibility rules in the editor and split large lighting/material/
  render modules into focused owners.

The old notes date this milestone September 21; its matching implementation
commit is September 22. No separate 0.5.3 package bump/tag is verified.

## 0.5.2 — 2026-09-21

- Ship complete **Office** and **Pool** environment families: final surface art,
  furniture, Pool curtains/guardrails/ladder, fixture families and showcase maps.
- Add wall-mounted lights, file-backed PNG decals and the final NO DIVING sign.
  Show a real recessed basin through the shared vertical geometry system.
- Correct decal orientation/atlas placement and author proper collision envelopes
  for the showcase's solid props. Water/swimming is not part of this milestone.

## 0.5.1 — 2026-09-21

- Replace hardcoded surface artwork with cataloged external PNG textures and
  materials, including authored tiling/tint, caching and custom pack resolution.
- Batch surfaces by material and share GPU textures; keep wall artwork upright
  and remove runtime procedural surface-texture generation.

## 0.5.0 — documented geometry milestone, 2026-09-21

- Add room elevations, raised/recessed floor regions, real transition faces and
  flat/gabled ceilings through one rendering/collision/lighting surface model.
- Place props/decals on their actual floor heights and support shallow steps;
  correct wall clipping that used another room's ceiling profile.

Recorded between the asset-catalog and PNG-material stages; no separate 0.5.0
package bump/tag in the post-reset Places series is verified.

## 0.4.0 — 2026-09-21

- Introduce the authoritative logical asset catalog, independent file locations,
  Office/Pool themes, distinct entity assets and catalog/reference validation.
- Keep legacy prop IDs and level references working while moving Spooner-Man
  into the shared entity organization.

## 0.3.2 — 2026-09-21

- Adopt WASD movement and arrow-key looking with matching menu navigation and
  reset defaults; preserve user rebinding and existing settings.

## 0.3.1 — 2026-09-21

- Move Places into its standalone repository/workspace while preserving the
  Vitrallis App Center application identity and compatibility paths.

## 0.3.0 — 2026-09-21

- Add authored, lit floor/wall/ceiling decals through a dedicated depth-biased
  pass, including editor selection, transforms and round-trip persistence.
- Resolve coincident water-damage walls into one physical surface with material
  runs, removing the major overlay z-fighting failure.

## 0.2.0 — documented lighting milestone, 2026-09-21

- Add RGB fixture lighting, mixed-color illumination and matching editor
  authoring. Make unlit rooms dark while retaining fixture-driven brightness
  in sparse large rooms.

The milestone is recorded in the inherited changelog; no separate post-reset
0.2.0 package bump/tag is verified.

## 0.1.0 — 2026-09-21

- Prepare the first App Manager-ready Places/Liminal release with native ARM
  packaging, executable-relative resources, application identity and licenses.
- Add **The Residence**, **Quiet Apartments** and **After the Leak**, with
  large authored residential layouts and progressive environmental decay.
- Add room/wall/ceiling material overrides and floor patches; preserve material
  boundaries when merging walls and remove overlapping patch slabs.

Versions 0.1.0–0.6.0 above use the inherited source-history dates, not verified
GitHub publication dates. See the [version/evidence audit](docs/reports/changelog-version-audit-2026-11-04.md)
for source commits and attribution gaps.

## Earlier prototype numbering — 2026-09-20 to 2026-09-21

These development labels precede the version reset to the first App Manager-ready
0.1.0. They are separate from the later releases with the same numbers. The
[original detailed notes](https://github.com/csd113/Places/blob/3fee560e484d5dbc61496b34c90cdef8292dbe2b/CHANGELOG.md#development-history-pre-release)
are the source; individual published releases are unverified.

| Prototype version | Main features, significant fixes and optimizations |
| --- | --- |
| 0.8.0 — Sep 21 | Spatial partitioning/frustum culling, indexed meshes and packed 24-byte vertices; reduced model-build work and fixed ignored VSync. The recorded 400-chair vertex buffer falls from 7.46 to 3.54 MB; this is a buffer result, not a general FPS claim. |
| 0.7.0 — Sep 20 | Reworked core props and tiling surface artwork; maintained/water-damaged variants and a stained ceiling. Reduced total core-prop triangles while improving geometry. |
| 0.6.0 — Sep 20 | Lighting audit/parity coverage; cached corner samples, fixture candidate filtering and flat-cell merging. Corrected doorway light leaks/reveals and removed duplicate startup level construction. |
| 0.5.0 — Sep 20 | Static baked lighting with room baselines, local fixture pools, doorway blending and world-space prop illumination; matching editor preview. |
| 0.4.0 — Sep 20 | Walkable Asset Demo covering the complete prop catalog, openings, fixture placement and material variants. |
| 0.3.1 — Sep 20 | Appearance/UV refinement only; no separate major feature recorded. |
| 0.3.0 — Sep 20 | Spooner-Man cat prop and reusable model-generation primitives/tooling. |
| 0.2.0 — Sep 20 | Real GLB core prop pack, cached model decoding, batched prop rendering and derived editor previews; corrected asset-path resolution. |
| 0.1.0 — Sep 20 | Browser 2D/3D editor, wall openings, catalog props, transform/history tools and validation/export; corrected broken undo/redo, saved materials and pack loading. |

## Historical PocketCHIP edition — independent version series

The [App Center mirror](https://github.com/csd113/Places/blob/b208b4647d20/apps/places-pocketchip/CHANGELOG.md)
was imported on September 26 and later retired from the desktop tree. Its
version numbers and hardware-specific results must not be attributed to the
October desktop releases. Earlier shared milestones are covered above.

| PocketCHIP version | Main features, significant fixes and optimizations |
| --- | --- |
| 0.11.1 — 2026-09-26 | ARMv7 patch package retaining power-of-two fitted artwork and correcting the tiled Pool ceiling seam. |
| 0.11.0 — 2026-09-25 | App Manager ARMv7 distribution; repair wall/reveal/stair/ramp winding and exposed wall caps, then enable safe back-face culling with explicit two-sided panes. Recorded device renderer work falls 1.4–1.9 ms. |
| 0.10.0 — 2026-09-25 | Prop albedo atlas and merged draws reduce the measured prop pass from 27 draws/binds to one; renderer work falls 1.1–2.0 ms with additional build time/memory. Presented median improvement remains inconclusive. |
| 0.9.0 — 2026-09-25 | Reduced-tier vertex fog, stacked lightmap texture, gated emission and fewer redundant state changes; correct the inverted no-lightmaps option. Recorded VSync-off reference frame time falls from 61–65 to 37.4–37.9 ms. |
| 0.8.0 — 2026-09-25 | Device-sized cached textures, simpler shader/profile, fewer batches and direct framebuffer rendering remove the reproduced offscreen GPU-fault path. Device-specific defaults/build tuning; recorded resident memory falls from 191 to 66 MB. |
