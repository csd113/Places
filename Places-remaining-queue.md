# Places: remaining work after the pause

**Snapshot:** November 21, 2026. **Queue status: paused.** Published baseline: [`0.14.0-dev`, `938c0b18`](https://github.com/csd113/Places/commit/938c0b186df2eb9c4f9fb992d4e778bb09331c88). This is a copy of the remaining queue, not an instruction to resume it. “Finding” refers to the internal code-review numbering; a linked “GitHub #” is a separate issue.

The opening slot, deferred issue order, and lower-priority feature order are recorded explicitly. Some held fixes were set aside without a new exact slot; they are listed in the technical block below, **not** newly prioritized by this snapshot. Eligibility matters: do independent work if a fixture, permission, or validation prerequisite is missing. Don’t close a partially verified issue.

## First on resume

### 1. NPC wander bounds and one-way doors (findings 50–51)
**Status:** Unpublished; first after an explicit resume. Fix together because the door retreat is one of the ways an NPC can escape its wander radius.
- **50: Wander radius.** Enforce the authored horizontal radius after navigation projection and on every actual motion path, including empty-path physical fallback and retreat from a door. Use the 0.65 m wanderers in the editor-authored Performance Lab. Verify multiple distinct in-range goals and actual movement, not just an idle NPC.
- **51: Door stand-off and route changes.** For a one-way door that swings toward the NPC, allow a physically clear retreat out of the door’s portal cells without making a real obstruction passable. Choose a reachable stand-off and respect the current route leg. Clear the remembered opening door when a replan changes the leg. Test realistic portal coverage, an actor already in it, a real obstruction, and a changed route.
- Keep loose physics-prop behavior unchanged; [GitHub #60](https://github.com/csd113/Places/issues/60) is separate and isn’t included. The earlier combined patch was held at tree `a477126e`; 47–49 were published separately and should not be reapplied.

## Held technical fixes and validation prerequisites

### 2. Player movement and swimming collision (finding 35)
**Held at `b6ec13d1`; no push.** A shallow prop-side hit can stop an upward movement sweep before the actual underside. During a water exit or swim fallback, the code may then leave upward velocity or exit state intact without recognizing valid ceiling contact. Resolve any safe remaining motion after the side hit and only report a ceiling the accepted movement can reach. Cover mixed side/underside contact, side-only ascent, real roofing, normal diving after a rejected exit, dormant floor support for jumping, and ladder controls. Keep the published prop-release fix (finding 36) intact.

### 3. Luau scheduling and terminal events (findings 43 and 45)
**Held at `a14dffc3`; no push.** Keep recurring module updates fair under shared instruction and event limits, including a full linked-event queue. Preserve the current tick's `dt` and a fresh, non-repeat F1 press. Once a one-shot timer or sequence completion is admitted, don’t lose linked script actions if the budget runs out. Resume at the first unexecuted action without replaying earlier side effects; commit once/cooldown only on completion. A timer stopping or replacing itself must not erase an already-running binding, and preserving a continuation must not drop a synchronous child event that previously fit. Preserve repeat timing, true stale-event cancellation, and the 4,096-event FIFO cap. Finding 44 is already published.

### 4. Exact trigger crossing positions under pressure (finding 46)
**Held separately at `7f1d72e8`; no push.** When a swept exit cannot be queued immediately, retain its solved crossing point rather than substituting a later outside position. Preserve exit → reentry → exit order where capacity allows, including crossings before the queue drains. The last candidate bounded storage per volume only, which could still retain millions of records across a large world. Add a real global bound and defined overflow behavior, while preserving occupancy consistency and lifecycle/reset rules. Don’t reintroduce the held Luau scheduler patch just to land this fix.

### 5. Editor Add Level transaction and restart behavior (finding 34)
**Narrow Add/Import safeguard published; broader recovery not implemented.** The compact Mac save/reopen check passed. Don’t redo the published patch. Keep automatic restart recovery distinct from in-session rollback: the current patch does not create a journal or promise crash recovery. Verify the remaining exact compiler/release and interrupted-publication gates where not covered by the compact run; don’t mark automatic restart recovery complete. This is not GitHub #34 (HUD).

### 6. Compiler asset identity and exFAT (finding 39)
**Source fix published at `d282738b`; remaining validation only unless a defect is reproduced.** Parsed catalog/model inputs must match the captured bytes. Relevant asset changes must invalidate the prior preparation result, while presentation-only edits should still reuse appropriate caches; interrupted or mismatched builds must preserve the previous valid output. Compact Full-lighting rendering passed, but it didn’t prove the exact changed-sky/cache comparison. Check unchanged reuse, changed-sky warm vs forced-fresh decoded reflection output, and prior-output survival where feasible.
- **exFAT:** Check Add/Import on an already-authorized disposable exFAT volume. No formatting, mounting, or drive changes to manufacture a test. Import uses a hard-link path that may fail there; that’s an open test, not a confirmed result. APFS and the compact editor test do not satisfy it.

### 7. Stock main menu/package blockers (part of GitHub #55)
**Reproduced on the published Mac build.** The stock Menu package expects a different mannequin GLB, so the player rejects it even though the game itself boots with a compatible fixture. Strict currentness separately rejected `closet_folding_door.glb` as outside the captured asset set.
- Treat these as two checks: runtime archive admission versus strict compiler currentness. The animated closet door may be read during level validation before ordinary model-dependency capture; confirm rather than assume. Fix capture without weakening exact-byte identity.
- Once bake-affecting fixes settle, rebuild only the needed Menu package through the supported workflow, then verify strict currentness and a bare-executable boot/return against the actual shipped assets. The successful compact fixture is not stock Menu acceptance.

### 8. Remaining lighting Clippy cleanup before final bakes
Audit the broad allowances in `src/engine/lighting/transport.rs` and the indexing suppression in `probes.rs`. Expose real strict diagnostics, fix or narrowly justify them, and preserve math, traversal and accumulation order, fallback behavior, and valid-dark results. Use checked index flattening and lookups where warranted. Compare one compact editor-authored Static/Full payload before and after if applicable; keep compiler identity changes out of old benchmark baselines. The inherited AI duplicate lint cleanup is done. Handle any residual non-bake suppressions at their later slot, not by widening the lighting pass.

### 9. Published gameplay changes: outstanding native witnesses
**Not another implementation pass.** Published NPC findings 47–49 and the prop-release change passed focused tests. Mac player and Quick Test confirmed cup pickup/release, but the exact NPC pursuit-replan deadlock, blocked scripted movement/reissue, and wrong-floor cases still need representative native gameplay. Use compact editor-authored fixtures. Record which cases actually ran; a native build, good GPU startup, or focused Rust test is not a substitute for the gameplay witness.

## Deferred maps and issue closeouts

### 10. Rebuild and verify only the maps needed for the remaining checks
Settle compiler changes first. Pin one compiler revision and matching assets; validate provenance, package integrity, and intended quality. Deduplicate requested checks across issues and use compatible evidence where available, but don't keep disposable bakes or rebuild a map merely to regenerate a report.
- **Backrooms:** Prior Razer Full bake stopped around 52% of direct work after about 54 minutes without an archive or resume checkpoint. Don’t blindly repeat the same run; only run a bounded or materially improved attempt if it still gates an authorized check.
- **Quiet collection:** One current nine-member Full collection should support Arcade UV and ordinary-wall control, elevator destination/return, and relevant memory/transition comparisons. Don’t repeat eight separate bakes if one qualifying collection will do.
- **Beam, The Long Way Home, and bundled Menu:** Reuse exact compatible evidence or refresh only what a specific acceptance check requires. Menu is the only current level slated to ship; other current maps are test fixtures, not permanent content.
- **Danger:** Exact Full cross-room floor-band/camera/blocker and matched native route only where authorized. A stale package or substitute room doesn’t count. Don’t retry any previously denied broad campaign without applicable authorization.
- Run eligible dependent checks after their maps are verified, before lower-priority features. Independent compact checks can run sooner.

### 11. [GitHub #27: Engine polish and performance](https://github.com/csd113/Places/issues/27)
**Open; movement row already verified.** Remaining: broader uncapped scenario and quality comparisons; longer resource-lifetime comparisons; shared limit/budget inventory, low-memory calibration and controlled allocation/over-limit refusal; comparable archive, installed/cache, and live-residency accounting. Investigate unexplained memory differences with matched inputs rather than treating all earlier results as missing. Re-evaluate the held benchmark-only patch against the current public branch before publishing it. No Windows/Linux sign-off in this Mac acceptance lane.

### 12. [GitHub #28: Game polish and performance](https://github.com/csd113/Places/issues/28)
**Open; published elevator work and quest recovery aren’t a reason to replay completed checks.** Finish the missing native interruption, pursuit, trigger, door, elevator/transition and long-session resource scenarios; final subjective audio listening; and matched archive/installed/runtime sizing and applicable packaging checks. Keep saved campaigns isolated. Do a real direct-binary lift retry when possible; leave .app-only Focus Companion or Gatekeeper paths discarded, not falsely passed.

### 13. [GitHub #29: Editor polish and performance](https://github.com/csd113/Places/issues/29)
**Open.** Fill gaps in remaining edit-operation and cold open/readiness timing; exercise longer mixed authoring/undo/map switching and resource lifetimes. Include relevant Lantern cold open and Hub switch when inputs are suitable. Compare compressed, logical, installed/cache and runtime sizes and export/load peak memory. Preserve save/undo and validation behavior; don’t retime the completed wall/history subset just to fill the report.

### 14. [GitHub #30: Map compiler polish and performance](https://github.com/csd113/Places/issues/30)
**Open; prior Lantern player peak was separately dispositioned.** Complete missing stage/CPU/wall/peak, copy/staging/disk and installed-cache measurements and broader repeat build/cancel/editor export acceptance. Recover an exact historical Lantern compiler baseline only if available; a new run can’t backfill an absent historical measurement. Use short reliable fixtures first, then a necessary matched larger check. CPU remains default; no GPU implementation.

### 15. [GitHub #31: Larger performance work](https://github.com/csd113/Places/issues/31)
**Open.** Qualify the existing GPU pose path, sockets/attachments, shadows and reflections across actual supported configurations; complete hidden-room/portal cases with doors, animation extremes, carried props and reflected/shadow views; and test streaming readiness, interruption, memory overlap and retirement. Only change architecture for a measured, reproducible gap. Keep source work separate from large-map sign-off missing a fixture.

### 16. [GitHub #55: Lighting rewrite final acceptance](https://github.com/csd113/Places/issues/55)
**Open; 18 of 31 checklist rows were recorded as checked.** The runtime and compiler already use the new lighting path; this is not a request to redo the rewrite. Settle the Menu and map gates above, then verify matched visual and performance coverage: seams/corners, floors/openings and camera distance, thin surfaces, moving/posed receivers, carried props, door/light-state changes, reflections, emissive/blacklight, Full water/caustics, transparency/fog/post-processing and editor-to-player behavior. Preserve valid darkness rather than compensating with arbitrary brightening. Check whether anything genuinely lighting-owned still sits outside `src/engine/lighting/`, including embedded shaders and compiler/editor/game call paths, without moving unrelated render bindings or regenerating the retired inventory. Retire obsolete code only once replacement behavior is supported. Some historical pose/layout/camera assets are missing; report those limits rather than claiming parity or reconstructing evidence. Reconcile issue checkboxes against current receipts and close only if all applicable gates pass.

### 17. Python/tooling follow-up 03, conditional
**Held on Windows permissions.** The first tooling fixes, ZIP comparison, and STEP follow-up are already shipped. Revisit only the specific unresolved Windows ACL case if still applicable and authorized; don’t replay the completed or archived proposals, broaden platform validation, or change security settings to unlock it.

## Post-pause cleanup and feature tail

### 18. Reduce Places to three buildable executables
Keep exactly `places` (game), `places-editor`, and `places-map-compiler` (rename existing `places-compile`). Remove extra build targets and their exclusive code, flags and dependencies rather than hiding them behind aliases or features. Put diagnostics worth retaining on the owning binary as `--debug <mode>`, off by default. Update build/package scripts and CI, then check Cargo metadata, package contents and relevant CLI behavior. No separate debug binaries or task writeups.

### 19. [GitHub #32: Spatial audio](https://github.com/csd113/Places/issues/32)
Finish subjective Office/Pool material and wall/door listening, moving listener/emitter checks, headphone front/back/elevation and speaker stereo fallback. Verify authored settings and any applicable Luau through editor → export/import → runtime, then remaining longer device/resource/performance cases. Don’t rebuild the audio foundation or re-run the bounded lifetime work already covered.

### 20. [GitHub #34: Optional HUD, health and melee](https://github.com/csd113/Places/issues/34)
Keep the existing six-HP player, three hearts, disabled-by-default HUD and crowbar behavior. Finish human NPC defaults (six HP, honoring lower authored values and immortality), monster defaults (generally at least 18 HP with design-specific balancing), and the flashlight indicator (visible only on an enabled map while on, no off-state placeholder). Verify repeated transitions/reloads/toggles without stale HUD state. Keep HUD visibility independent of gameplay enablement.

### 21. [GitHub #43: Technical architecture and API documentation](https://github.com/csd113/Places/issues/43)
The issue explicitly requests documentation, but the newer standing direction is **no routine task reports or reference sheets**. Reconcile that before writing. Cover current engine/game/editor/compiler responsibilities, shared ownership and important data contracts, and the real editor → compiler → package → player path. Verify links/examples against current code and prefer concise maintained content to copying the issue’s historical multi-document scope verbatim. This isn’t an architecture rewrite.

### 22. [GitHub #42: Four Office/Home props](https://github.com/csd113/Places/issues/42)
Office: tapered grey open wastebasket and chunky cream floor-standing photocopier with dark lid and simple trays. Home: rounded cream swing-lid kitchen garbage can and a ceiling fan with wide wood blades and a compact light. Keep Places' low-poly look, clear silhouettes, plausible scale and sensible collision. Use Blender/GIMP on the assistant’s desktop for asset creation, add assets to the proper catalogs, and check their appearance under native lighting. Only these four are approved; animation and interaction aren’t implied.

### 23. Independent audio sliders
Under Settings → Audio, add or reuse Master, Ambient and Prop Sound volume sliders. Master affects all audible channels; category sliders affect only their own sounds. Use a small extensible category/mix design, preserve authored gain and spatial attenuation, clamp and persist settings, and retain defaults when older settings lack the fields. Check active loops and new sounds, another channel under Master, and save/reload using a compact Mac scenario.

### 24. Static audit of all Settings tabs
After the audio sliders, inspect grouping, order, labels, duplication, buried controls, discoverability, keyboard/navigation/accessibility concerns, and source organization. Give prioritized concrete improvements, distinguishing actual behavior from source inference and preference. This is read-only; no redesign or code change.

### 25. Broader documentation cleanup
After the pause only. Remove obsolete and redundant task reports, reference sheets, and stale duplicated docs while keeping essential user, authoring and asset instructions. Include the redundant root `LIGHTING_CODE_INVENTORY.md` in the repository cleanup; its local removal is a separate action. Don’t resurrect old private history or add a new archive of reports. Use one rolling handoff for actionable open work only.

### 26. Rust 1.100 follow-ups
- **Cargo unused dependencies:** After the three-binary cleanup, check the remaining targets/features. Remove only conclusively unused workspace dependencies; keep valid editor/compiler-only, optional and target-specific dependencies.
- **`DropGuard`:** Inspect save/export staging for narrow cleanup-on-early-exit opportunities. Keep explicit rollback where failure must be reported; don’t rewrite working transactions just to use a new API.
- **Allocators:** Investigate only if compiler profiles show a material cost for allocating/freeing short-lived objects. Benchmark against a matched baseline and preserve parity before any adoption. No speculative allocator or never-type refactor.

### 27. Move HVAC duct join metadata into the editor
Move `tools/props/backrooms_phase6/duct_join_metadata.json` beside `src/editor/ducts.rs` or into an editor-owned data subdirectory. Update the `include_str!` path and any live tooling paths. Preserve the JSON bytes and existing duct snapping behavior. Verify a focused `places-editor` build and a representative duct join through preview and apply.

### 28. Remove the `tools/` directory
The goal is to delete `tools/` outright, not tidy it or move it wholesale. Treat legacy files as removable by default. Delete obsolete Blender/Python model and asset generators and their exclusive helpers, fixtures, benchmarks, and tests in bulk. Current models were authored by hand; do not regenerate or audit the assets. A reference from another old script, test, or CI job is not evidence of a current need.

Start from what the current game, editor, and map compiler actually need to build, run, and ship. If deletion exposes a real dependency, move only the live data beside its owner or replace only the necessary behavior. Prefer direct Cargo and simple platform-native packaging over porting Python scripts wholesale. Drop checks that only preserve retired tooling; keep checks that protect current behavior. Reuse the three-binary and duct metadata work above rather than doing it twice.

Verify the three supported entry points and the affected build and packaging paths with focused checks. Leave no replacement catch-all folder, archive of deleted tools, or task report.

### 29. Hand-build a compact Model Zoo showcase
Build a small, walkable gallery by hand in the Places level editor. Place one example of every current editor-available model, prop, decal, wall texture, floor texture, light type, and other placeable asset or surface category. Group related models close together and use small wall and floor swatches. Keep aisles short and leave only the clearance needed to walk around and see each display. Avoid oversized rooms, empty wings, and long corridors.

Treat it as a snapshot of the current catalog to show friends, not a test fixture. No automated placement, generated map edits, test suite, coverage checks, or automatic updates when the catalog changes. Save the editable map and export a standalone `.placesmap` separately; do not bundle it with the base game or put it in the default map list.

## Working rules and exclusions

- One source writer at a time, one checkout per machine, fresh task context for distinct work where practical. Use the authorized dev branch; no force push, main/release work, or unrelated edits. The High setting was for the old pre-pause work; normal source implementation was Sol 6.2 Extra High unless changed, with an independent Sol 6.2 Medium static review for exact changes.
- Native acceptance uses Mac/Metal and direct executables. Author levels only in the game editor; the built-in compiler can build editor-saved levels. Use narrowly scoped tests first, and don’t carry over disposable builds, logs, or bakes at task closeout. Keep only actual user work or live inputs.
- Not remaining implementation items: completed FPS/compiler rounds, GPU bake feasibility (GitHub #36 closed not planned; separate Windows RTX research issue is outside this queue), test-pruning review (recommendations only, no deletion authorized), PocketCHIP, zombie texture, GitHub #50/#51 furniture reports closed or dispositioned separately, GitHub #52, and source split recommendations in GitHub #56. Don’t confuse closed furniture GitHub #50/#51 with pending internal NPC findings 50/51. GitHub #35 and #53 were not selected here; #60 is not part of the NPC batch.
- The independent Mac/Razer disk cleanup and any Mac Git-storage repair are separate from this code queue. No item in this document restarts the queue.

