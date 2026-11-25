# Places: remaining work after the pause

**Queue status.** Active through item 10. Pause before item 11 and the shared large-map bakes. If item 10 needs a shared bake, leave only the bake-dependent check pending. Resume after the pause only when Connor explicitly asks. Work on `0.14.0-dev`. Sync to the current published branch and check issue state before starting. Finding numbers refer to the internal review.

## Harness instructions

1. **Implement with Sol 6.2 High by default.** Use Extra High only when needed. Use a fresh Codex context for each item, with its scope, dependencies, known evidence and acceptance checks. Hand over narrow diffs and exact paths. Keep one source writer at a time and one canonical checkout per machine. Use independent agents only where work does not overlap.
2. **After every item, run a separate Sol 6.2 Medium code audit.** Use a fresh reviewer context and the exact diff or commit. Review correctness, idiomatic Rust, regressions and whether any new tests protect current behavior. For research, documentation or map-only items, review the actual deliverable and confirm there were no unintended code changes; do not invent a test suite. Add actionable findings to the **next runnable queue item** with the affected paths and the required check. If a finding blocks acceptance, keep the affected issue open until fixed and verified. If there is no next item, append a short follow-up.
3. **Mark off completed work on GitHub.** After focused validation and independent review, commit and verify the push to `0.14.0-dev`. Check off the queue item in this file, update the corresponding GitHub issue checklist and close the issue as completed only when its applicable scope is finished. Do not close a multi-item issue from a partial pass or treat an unverified fix as complete. For items without a linked issue, the GitHub queue file is the record; do not create a ticket just to close it.
4. **Purge task temp files immediately after closeout, including failed or canceled runs.** Remove disposable bakes, caches, staging directories, logs and generated scratch files from the machine where they were made. Run `cargo clean` on the exact task-owned, idle target/build directories once deliverables are out of them. No archives, duplicate caches or recovery indexes for reproducible output. Preserve source work, requested deliverables and only the exact inputs an unfinished dependent item still needs; purge those temporary inputs after that dependency is done.
5. **Avoid rework.** Prefer compact editor-authored fixtures and narrow checks. Use the direct executable, not an `.app`, for Mac playtests. Author maps manually in the editor; the built-in compiler may export editor-saved maps. Keep required large bakes together behind their compiler fixes; move only a blocked large-map check into that pass, not the independent code work. If a reported bug cannot be reproduced after a bounded check, record that outcome and keep moving. Do not turn full Windows/Linux validation into a gate for this queue. No force pushes, merges to main, releases, routine writeups or new reference documents unless explicitly requested.

## Release blockers and compact checks

### 1. NPC wander bounds and one-way doors (findings 50–51)
**Complete.** Code and independent Sol 6.2 Medium review checked at `09f7a8a531380114493bf402ce89ab6316731c5b`. Findings 50 and 51 are internal review entries.
- **50: Wander radius.** Enforce the authored horizontal radius after navigation projection and on every actual motion path, including empty-path physical fallback and retreat from a door. Use the 0.65 m wanderers in the editor-authored Performance Lab. Verify multiple distinct in-range goals and actual movement, not just an idle NPC.
- **51: Door stand-off and route changes.** For a one-way door that swings toward the NPC, allow a physically clear retreat out of the door’s portal cells without making a real obstruction passable. Choose a reachable stand-off and respect the current route leg. Clear the remembered opening door when a replan changes the leg. Test realistic portal coverage, an actor already in it, a real obstruction, and a changed route.
- **Evidence.** Prior Mac native checks observed 12 moving Performance Lab wanderers within their 0.65 m radius and exercised a real door obstruction and alternate route. The exact follow-up cases, including the ordinary wander detour, projected home recovery, scripted Stay and Return Home, and the last-slot door retry, passed in the 39 focused Razer Rust tests. Strict Clippy and the locked all-features release build passed. No new native gameplay run covered the exact follow-up cases.
- Keep loose physics-prop behavior unchanged in this batch; [GitHub #60](https://github.com/csd113/Places/issues/60) is queued separately. The earlier combined patch was held at tree `a477126e`; 47–49 were published separately and should not be reapplied.

### 2. Published gameplay changes: outstanding native witnesses
**Open for native gameplay verification only.** Published NPC findings 47–49 and the prop-release change passed focused tests. Mac player and Quick Test confirmed cup pickup/release, but the exact NPC pursuit-replan deadlock, blocked scripted movement/reissue, and wrong-floor cases remain unverified in native gameplay. Use compact editor-authored fixtures. Record which cases actually ran; a native build, good GPU startup, or focused Rust test is not a substitute for the gameplay witness.

### 3. Player movement and swimming collision (finding 35)
**Complete.** Finding 35 was implemented at `0a583b0e3ac41df1393f4db9b7639a512027a9df` and passed independent Sol 6.2 Medium re-review at `d51111fd431a5a3106b6efac59ee6066f88b546a`. Swimming preserves safe ascent after a prop-side hit and stops at a reachable underside. A dormant prop over a rim prevents a blocked exit from restarting during a dive. Held-item slides preserve safe tangential motion and the accepted release velocity.

All 18 focused Rust tests passed, including dormant floor support, ladder controls, and the finding 36 prop-release regression. Strict Clippy and the locked all-features release build passed. The serial workspace suite passed 663 tests with the known NPC carry-reset case excluded. Razer Pool Quick Test at `0a583b0` confirmed ladder climb, idle hold, backward detachment, and diving. CPU controller tests covered the exact mixed contacts and both follow-up regressions.

### 4. Luau scheduling and terminal events (findings 43 and 45)
**Complete.** Findings 43 and 45 passed independent Sol 6.2 Medium re-review at `ae136f65926d362bc99b9dd7a2ef55347340e676`. Recurring Luau updates share frame limits fairly and use the current tick's `dt`. A fresh, non-repeat F1 press uses the new frame's shared budget. Admitted one-shot timer and sequence-completion bindings resume at the first unexecuted action, retain their first synchronous child's FIFO slot, and commit once and cooldown on completion. Linked sequence steps get a turn under sustained budget pressure. The checks covered repeat timing, stale cancellation, and the 4,096-event FIFO cap. Finding 44 is already published.

On the Razer, all 22 targeted tests, scoped strict Clippy, and the locked all-features player build passed at the reviewed commit. The preceding audit-fix commit `ced5af4e4dbca1c38dae4e98d0481999c4a38057` passed strict workspace Clippy, the locked all-features workspace release build, and 670 serial workspace tests with the known NPC carry-reset case excluded.

### 5. Exact trigger crossing positions under pressure (finding 46)
**Complete.** Finding 46 passed independent Sol 6.2 Medium re-review at `4e1feb7069d6cad5ad5d3c94bc412a42e4d8abdc`. Trigger sweeps report crossing positions along the solver-accepted path. The global 4,096-event queue admits crossings in chronological order, preserves FIFO under budget pressure, and drops the newest crossing when full. Occupancy follows the solved motion even when delivery is dropped. A dropped exit stops the visit’s loop when an authored stop identifies the emitter. Cleanup does not replay exit actions, and checkpoint restore preserves the active cue for owned loops.

On the Razer, the four focused movement tests passed at the reviewed commit. They covered the wall slide, accepted vertical peaks, the side-only prop clip, and the crossing order at the last FIFO slot. The mixed-motion test also confirmed that a volume between the solved legs produces no events. Strict workspace Clippy and the locked all-features workspace release build passed.

### 6. Editor Add Level transaction and restart behavior (finding 34)
**Complete.** Independent Medium re-review passed `697df9f33b471a6f40d4072080570f14b512b0ef` and `3382d77ee060249d9f010b3027ee0b3d93d0c30e`. The editor resumes a pending Add/Import on reopen when it can verify the source and manifest. On disagreement it preserves the prior project and staged originals. A committed record is cleared only after the published level source and required mode copy match the expected bytes. An in-session failure persists a cancellation marker before cleanup. If the editor cannot persist cancellation, it reports that the outcome is uncertain and reopening may still complete the operation.

On the Razer, the compact one-room compiler smoke at `3382d77e` passed export, strict currentness, and package validation for the `off` lighting variant. At `697df9f3`, strict workspace Clippy, the locked all-features release build, and all six recovery tests passed. The tests covered changed readable source bytes, missing and different mode copies, and restart after blocked cancellation or cleanup. The serial workspace run had 688 passes and the inherited carry-reset failure. The earlier compact Mac save/reopen check also passed.

### 7. Compiler asset identity and exFAT (finding 39)
**Compiler validation complete.** The source fix was published at `d282738b`. Native Razer validation at `09d57e29a7e77ec632ff18e4482912a2942ea350` confirmed that an unchanged build reused the cache and preserved the package modification time. After a sky change, warm and forced-fresh builds produced byte-identical packages and matching decoded reflections. Cancellation and failure preserved the prior outputs. Independent Sol 6.2 Medium review `6b04bb8e-79d0-83e9-b987-3f6f074c9005` cleared the compiler work.
- **exFAT deferred.** When resumed, check Add/Import on an already-authorized disposable exFAT volume. No formatting, mounting, or drive changes to manufacture a test. Import uses a hard-link path that may fail there; that’s an open test, not a confirmed result. APFS and the compact editor test do not satisfy it.

### 8. Stock main menu/package blockers (part of GitHub #55)
**Complete.** The compiler captures the folding-door GLB before rig validation and verifies its exact bytes through publication. The stock package was rebuilt with the built-in compiler from the unchanged editor-saved Menu project. On the Razer, strict currentness had zero differences and all 43 installed dependencies matched. The bare player rejected the old package on the mannequin hash, then presented the rebuilt Menu at startup and again after returning from gameplay through the pause menu.

Independent Sol 6.2 Medium review `6b04c98c-0230-83e9-9e67-7f14173ff002` cleared the code, package, and native evidence. Strict Clippy, the locked all-features release build, and all six focused tests passed. [GitHub #55](https://github.com/csd113/Places/issues/55) remains open for the broader lighting acceptance.

### 9. Remaining lighting Clippy cleanup before final bakes
**Complete.** Tightened the lighting lint exceptions and made probe indexing checked while preserving math, traversal and accumulation order, fallbacks, and valid-dark output. Strict workspace Clippy, the locked all-features release build, and 18 focused tests passed. All 21 compact Static/Full payloads matched byte-for-byte, including 50 valid-dark results. Independent Medium review 6b04d374-0a24-83e8-8ad1-11be41a95015 cleared both files.

### 10. [GitHub #53: Reproduce and fix the Long Way Home FPS drops](https://github.com/csd113/Places/issues/53)
**Reproduced on Danger’s second elevator exit. Cause and fix remain open.** Two fresh-state Razer runs at `8b908398` used the verified Long Way Home archive and reproduced the authored first-exit encounter, re-entry, and second exit. First-exit averages were 207.0 and 203.3 FPS. Second-exit averages dropped to 91.6 and 87.6 FPS with 46.7 and 48.0 ms p99 frame times. The logged repeat confirmed DX12 Immediate at a physical 1280×720, V-sync off and the frame limiter unset. Its second-exit CPU update p95 was 20.8 ms. Independent Sol 6.2 Medium review `6b055927-e3f0-83ea-b0c0-18d4cf5e1000` recalculated both intervals. Profile the CPU update to identify and fix the cause.

## Shared bakes and lighting sign-off

### 11. Rebuild and verify only the maps needed for the remaining checks
**Complete for shared exports.** At `c5e5dfd`, the Razer native editor exported
one three-member Long Way Home archive and one nine-member Quiet Collection
archive. All 12 members passed strict currentness and archive validation, with
Static and Full variants and a Full atlas for each. Backrooms source is archived
and its environment assets remain available. Independent review cleared the
export gate. The exact Danger gameplay check for #53 and lighting acceptance
for #55 remain open.

Settle compiler changes first. Pin one compiler revision and matching assets; validate provenance, package integrity, and intended quality. Deduplicate requested checks across issues and use compatible evidence where available, but don't keep disposable bakes or rebuild a map merely to regenerate a report.
- **Backrooms.** Prior Razer Full bake stopped around 52% of direct work after about 54 minutes without an archive or resume checkpoint. Archive the Backrooms level and remove it from the active compile and review pool. Keep the Backrooms environment assets available.
- **Quiet collection:** One current nine-member Full collection should support Arcade UV and ordinary-wall control, elevator destination/return, and relevant memory/transition comparisons. Don’t repeat eight separate bakes if one qualifying collection will do.
- **Beam, The Long Way Home, and bundled Menu:** Reuse exact compatible evidence or refresh only what a specific acceptance check requires. Menu is the only current level slated to ship; other current maps are test fixtures, not permanent content.
- **Danger:** Exact Full cross-room floor-band/camera/blocker and matched native route only where authorized. A stale package or substitute room doesn’t count. Don’t retry any previously denied broad campaign without applicable authorization.
- If #53 was blocked only on a fresh Long Way Home package, do its remaining reproduction here using a shared qualifying export. Carry any bake-affecting fix back through the relevant compact checks before pinning the final compiler. Run eligible dependent checks after their maps are verified, before lower-priority features.

### 12. [GitHub #55: Lighting rewrite final acceptance](https://github.com/csd113/Places/issues/55)
**Partial acceptance. #55 remains open.** Razer DX12 checks at `8b908398` reused the verified item 11 exports. Writer `6b05545e-e284-83ea-9859-8e82d1ab800d` and independent Sol 6.2 Medium reviewer `6b055927-e3f0-83ea-b0c0-18d4cf5e1000` support 21 of 31 checklist rows within the reviewed scope, adding representative scene coverage, Feltlings editor-to-player comparison, and the agreed Menu and shared-map rebuilds.

- Danger showed a continuous lower-floor gradient, a retained wall shadow, and a shaded duck through pickup, a 3.75 m carry, and release. Feltlings showed near and far animated poses and the thin door leaf at 0° and 90°. The moving guide’s lighting updated before a physical block stopped it short of the doorway.
- Arcade’s UV carpet changed by 18.3 RGB levels on average while ordinary-wall and emissive-screen controls changed by at most one level. Restoring the UV and mirror controls reproduced their original pixels. Pool showed moving Full caustics and restored the sampled receiver energy when the source was re-enabled.
- Feltlings Quick Test and the compiled player showed corresponding geometry and fixture placement. DX12 passed Full → Static → Full and Hub → Arcade → Hub, with zero retired prepared-build, record, and transport owners at the measured checkpoints.

Lighting ownership remains open for UV sampling and occlusion, switchable contribution grouping, caustics source and receiver evaluation, flashlight shadow bias and filtering, and mixed reflection policy in renderer code. Follow-ups also include controlled floor or passage separation, an animated doorway crossing with near and far views, tinted-glass and fog/exposure controls, matched-quality performance, and final architecture and checklist reconciliation. Historical poses, layouts, and cameras limit exact parity, and the standalone spec and historical bake peak-memory receipt were unavailable.

## NPC and map import follow-ups

### 13. [GitHub #60: NPCs pass through loose physics props](https://github.com/csd113/Places/issues/60)
After the NPC blocking fixes and their native checks, exclude only loose movable physics props from NPC path planning and movement collision. NPCs must not push the prop or wake it. Walls, doors, fixed props, actors, held props and player interaction keep their existing behavior. Verify on a compact editor-authored route with both loose clutter and a genuine obstruction; no mass thresholds or new authoring options.

**Complete.** In the Razer native run, the NPC passed one and two sleeping props (2 kg and 500 kg) without moving or waking them. Another actor and the locked door blocked passage. The unlocked door opened and allowed passage. Focused Rust checks confirmed held props still block NPCs and that a moving prop wakes a dormant prop through an NPC proxy while a closed door shields it.

Pursuit and wrong-floor behavior passed natively. Finding 48 passed its focused Rust test and remains unverified natively. Independent Sol Medium review, 23 focused tests, strict workspace Clippy, and the locked all-features release build passed. The workspace suite had 692 passes and the inherited NPC carry-reset failure.

### 14. [GitHub #62: Import multiple compiled maps from one local ZIP](https://github.com/csd113/Places/issues/62)
Build on the existing single-map importer. Find nested `.placesmap` members, ignore unrelated entries, and reuse normal per-map validation and conflict rules without overwriting installed maps. Bound archive expansion and entry count and reject unsafe paths without extracting arbitrary files. Valid maps may succeed despite invalid siblings; report imported and skipped items and reasons. Local compiled-map ZIPs only; no mods, map servers or automatic downloads.

**Complete.** The Razer native picker returned quietly to Explore on Cancel and showed per-file results for a selected ZIP. The direct player check verified nested import, invalid and incompatible siblings, ignored unrelated files, exact byte preservation on collision, and clear rejection of invalid, empty-of-maps, and over-limit ZIPs. The boundary tests checked the 256-entry cap and the streamed byte budget. No staging remained.

Independent Sol Medium review, strict workspace Clippy, the locked all-features release build, and 10 focused tests passed. The earlier serial workspace run had 694 passes and the preexisting NPC carry-reset failure.

## Build and repository cleanup

### 15. Reduce Places to three buildable executables
Keep exactly `places` (game), `places-editor`, and `places-map-compiler` (rename existing `places-compile`). Remove extra build targets and their exclusive code, flags and dependencies rather than hiding them behind aliases or features. Put diagnostics worth retaining on the owning binary as `--debug <mode>`, off by default. Update build/package scripts and CI, then check Cargo metadata, package contents and relevant CLI behavior. No separate debug binaries or task writeups.

**Complete.** Cargo metadata confirms exactly three executables. Independent Sol Medium review, strict workspace Clippy, and the locked all-features release build passed. The native Windows editor exported the saved menu through the renamed compiler, and Windows packaging verified the current menu and included all three executables with no gameplay maps. macOS menu routing and rollback were checked with shell fixtures, not on a native Mac.

### 16. Move HVAC duct join metadata into the editor
Move `tools/props/backrooms_phase6/duct_join_metadata.json` beside `src/editor/ducts.rs` or into an editor-owned data subdirectory. Update the `include_str!` path and any live tooling paths. Preserve the JSON bytes and existing duct snapping behavior. Verify a focused `places-editor` build and a representative duct join through preview and apply.

**Complete.** The editor now owns the byte-identical metadata. On the Razer, Preview left the source unchanged, Apply joined the equal-scale short and straight duct flanges, and one Undo restored the saved pose. The focused editor build and duct test, strict workspace Clippy, locked all-features release build, and independent Sol Medium review passed.

### 17. Remove the `tools/` directory
The goal is to delete `tools/` outright, not tidy it or move it wholesale. Treat legacy files as removable by default. Delete obsolete Blender/Python model and asset generators and their exclusive helpers, fixtures, benchmarks, and tests in bulk. Current models were authored by hand; do not regenerate or audit the assets. A reference from another old script, test, or CI job is not evidence of a current need.

Start from what the current game, editor, and map compiler actually need to build, run, and ship. If deletion exposes a real dependency, move only the live data beside its owner or replace only the necessary behavior. Prefer direct Cargo and simple platform-native packaging over porting Python scripts wholesale. Drop checks that only preserve retired tooling; keep checks that protect current behavior. Reuse the three-binary and duct metadata work above rather than doing it twice.

Verify the three supported entry points and the affected build and packaging paths with focused checks. Leave no replacement catch-all folder, archive of deleted tools, or task report.

**Complete.** Retired the tracked tooling and kept live workflows with their owners. The two unchanged Utility and HVAC Kit Showcase source files are preserved in the [map archive](archive/projects/hvac-utility-showcase/), excluded from default compilation and release packaging. The Windows package passed staged menu freshness, payload hashes, and headless checks of all three executables. POSIX packaging was checked with fixtures, not a native Mac run.

### 18. Rust 1.100 follow-ups
- **Cargo unused dependencies:** After the three-binary cleanup, check the remaining targets/features. Remove only conclusively unused workspace dependencies; keep valid editor/compiler-only, optional and target-specific dependencies.
- **`DropGuard`:** Inspect save/export staging for narrow cleanup-on-early-exit opportunities. Keep explicit rollback where failure must be reported; don’t rewrite working transactions just to use a new API.
- **Allocators:** Investigate only if compiler profiles show a material cost for allocating/freeing short-lived objects. Benchmark against a matched baseline and preserve parity before any adoption. No speculative allocator or never-type refactor.

**Complete. Independently reviewed.** All 21 direct crates have live callers and both feature switches are used. No narrow DropGuard change was justified. Save and export rollback stays explicit where cleanup failures must be reported. Existing compiler timings do not isolate allocation costs, so no allocator benchmark or change was warranted. On Windows, 20 locked Rust 1.100.0 package, archive and publication tests passed. No source or build inputs changed, and no new release build or map bake was run.

### 19. Python/tooling follow-up 03, conditional
**Held on Windows permissions.** The first tooling fixes, ZIP comparison, and STEP follow-up are already shipped. Revisit only the specific unresolved Windows ACL case if still applicable and authorized; don’t replay the completed or archived proposals, broaden platform validation, or change security settings to unlock it. Skip if the tooling removal makes it obsolete; Windows validation is not a release gate for this queue.

## Audio, HUD and environment work

### 20. [GitHub #32: Spatial audio](https://github.com/csd113/Places/issues/32)
Finish subjective Office/Pool material and wall/door listening, moving listener/emitter checks, headphone front/back/elevation and speaker stereo fallback. Verify authored settings and any applicable Luau through editor → export/import → runtime, then remaining longer device/resource/performance cases. Don’t rebuild the audio foundation or re-run the bounded lifetime work already covered.

### 21. Independent audio sliders
Under Settings → Audio, add or reuse Master, Ambient and Prop Sound volume sliders. Master affects all audible channels; category sliders affect only their own sounds. Use a small extensible category/mix design, preserve authored gain and spatial attenuation, clamp and persist settings, and retain defaults when older settings lack the fields. Check active loops and new sounds, another channel under Master, and save/reload using a compact Mac scenario.

### 22. Static audit of all Settings tabs
After the audio sliders, inspect grouping, order, labels, duplication, buried controls, discoverability, keyboard/navigation/accessibility concerns, and source organization. Give prioritized concrete improvements, distinguishing actual behavior from source inference and preference. This is read-only; no redesign or code change.

### 23. [GitHub #34: Optional HUD, health and melee](https://github.com/csd113/Places/issues/34)
Keep the existing six-HP player, three hearts, disabled-by-default HUD and crowbar behavior. Finish human NPC defaults (six HP, honoring lower authored values and immortality), monster defaults (generally at least 18 HP with design-specific balancing), and the flashlight indicator (visible only on an enabled map while on, no off-state placeholder). Verify repeated transitions/reloads/toggles without stale HUD state. Keep HUD visibility independent of gameplay enablement.

### 24. [GitHub #42: Four Office/Home props](https://github.com/csd113/Places/issues/42)
Office: tapered grey open wastebasket and chunky cream floor-standing photocopier with dark lid and simple trays. Home: rounded cream swing-lid kitchen garbage can and a ceiling fan with wide wood blades and a compact light. Keep Places' low-poly look, clear silhouettes, plausible scale and sensible collision. Use Blender/GIMP on the assistant’s desktop for asset creation, add assets to the proper catalogs, and check their appearance under native lighting. Only these four are approved; animation and interaction aren’t implied.

### 25. [GitHub #41: Maintenance environment](https://github.com/csd113/Places/issues/41)
Build an unfurnished concrete back-of-house environment: service halls and stairs, industrial basements and modular unfinished surfaces. Integrate and rename the existing Janitor closet content instead of duplicating it; migrate catalog/editor references without breaking authored maps. Create assets in Blender/GIMP where needed, build any map by hand in the level editor and validate representative traversal.

### 26. [GitHub #40: Underground Mall environment](https://github.com/csd113/Places/issues/40)
Follow the approved underground direction: polished tile and glass storefronts/balcony rails using existing effects, no daylight/skylights or fountain. Use the issue’s approved props and fluorescent lighting. Add six small movable pickup props: a scuffed plastic food-court tray, a chunky moulded clothes hanger, a lidded Starbucks-style takeaway coffee cup with green coffee-shop-style branding, a crumpled paper ball, a small metal ashtray, and a crushed cigarette pack (a soft pack with its top peeled back, placed beside the ashtray). The cup and paper ball are new silhouettes, not replacements for Arcade litter. Reuse the same escalator assets in stopped and working states; triggers toggle them and working escalators carry players, props and entities. Security shutters must open and close. Author and check a compact sample in the editor before expanding.

## Research only, then multiplayer

### 27. [GitHub #45: Research reusable animation graphs and masks](https://github.com/csd113/Places/issues/45)
Research only. Compare a small reusable graph/mask layer with existing clip transitions, crossfades and upper-body blending. Check whether it would simplify real Places use cases, including mannequin locomotion and tool overlay, without changing gameplay-owned pose or collision behavior. Report adopt, prototype separately, defer, or reject; no code changes or engine migration.

### 28. [GitHub #46: Research safe live asset reloading](https://github.com/csd113/Places/issues/46)
Research only. Compare safe editor-only asset refresh with current cache, preview and save/build/play paths. Identify behavior on malformed edits, pipeline and dependency invalidation, resource lifetime, and separation from published packages and script execution. Separate a full scene reload from an in-place refresh. No production file watcher or hot-reload implementation.

### 29. [GitHub #47: Research ECS scheduling](https://github.com/csd113/Places/issues/47)
Research only. Compare Bevy-style data-access-aware scheduling with the current Places entity loop and targeted parallelism. Ground recommendations in an actual bottleneck and preserve event order, determinism, and main-thread ownership. No Bevy migration or production scheduler implementation.

### 30. [GitHub #15: Multiplayer and in-session text chat](https://github.com/csd113/Places/issues/15)
**Deferred.** Leave GitHub #15 open. Keep this large feature behind core fixes and smaller work. Define a bounded join/host/leave and reconnect flow, synchronize movement and visible players, choose and document the shared interactions and world state, and add in-session text chat. Validate two players together and cleanup/reconnect under latency or disconnection. Do not broaden into unrelated engine or account work.

## Documentation and standalone showcase

### 31. Broader documentation cleanup
**Active after the pause.** Finish any remaining documentation cleanup without undoing the already-published consolidation. Keep the docs index, one summary per version, useful guides for the game, engine, editor and map compiler, and clearly named screenshots that serve a guide. Delete stale task reports and duplicated notes; no new archive or reference report. The root lighting inventory has already been removed; do not redo that work.

### 32. [GitHub #43: Technical architecture and API documentation](https://github.com/csd113/Places/issues/43)
**Deferred.** Leave GitHub #43 open. The issue explicitly requests documentation, but the newer standing direction is **no routine task reports or reference sheets**. Reconcile that before writing. Cover current engine/game/editor/compiler responsibilities, shared ownership and important data contracts, and the real editor → compiler → package → player path. Verify links/examples against current code and prefer concise maintained content to copying the issue’s historical multi-document scope verbatim. This isn’t an architecture rewrite.

### 33. Hand-build a compact Model Zoo showcase
Build a small, walkable gallery by hand in the Places level editor. Place one example of every current editor-available model, prop, decal, wall texture, floor texture, light type, and other placeable asset or surface category. Group related models close together and use small wall and floor swatches. Keep aisles short and leave only the clearance needed to walk around and see each display. Avoid oversized rooms, empty wings, and long corridors.

Treat it as a snapshot of the current catalog to show friends, not a test fixture. No automated placement, generated map edits, test suite, coverage checks, or automatic updates when the catalog changes. Save the editable map and export a standalone `.placesmap` separately; do not bundle it with the base game or put it in the default map list.

## Not in this queue

- GitHub #27–#31 are already closed as completed. Do not reopen, replay their old checklists or restore their work as separate tasks. Only overlapping checks independently required by an active issue still apply.
- GitHub #44 and #36 are closed as not planned. The separate Windows RTX study (#57), source-split recommendations (#56), and other unselected future features remain out of scope.
- Published PocketCHIP and zombie texture work, the completed FPS/compiler rounds, test-pruning recommendations and prior Mac/Razer cleanup are not new tasks. GitHub #50/#51 are furniture issues, not the internal NPC findings with the same numbers. Nothing here starts or resumes a run.
