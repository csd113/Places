# After stage 7: navigation pass, assets and authored modes

Stage 7 ends with complete project distribution/recovery. Its report and exact
local receipt are `docs/reports/editor-0.12.0-task7.md` and
`local/verification/0.12.0-task7/stage7-handoff.json`. Stay on `0.12.0-staging`.
No main merge, release, branch deletion, `.app` launch or roadmap stage 8/9 work
is authorized by this handoff.

## Runnable paths

Repository: `/Users/connordawkins/Documents/GitHub/Places`.
Preserved direct editor/compiler/player: `local/verification/0.12.0-task7/payload/`.
Open `local/verification/0.12.0-task7/try-editor.sh`; it selects the actual
`project/` authoring folder, explicit repository built-ins and isolated manual
state. `try-player.sh` imports/plays the retained complete package directly.
The host supplies SDL3. Do not use an `.app` wrapper. Build+Play calls the same
compiler/player subprocess boundaries and preserves the launch cwd.

`project/places.project.json` declares `stage7_entry` in
`levels/stage6_entry.json` and `stage7_second` in `levels/stage7_second.json`.
`project/scripts/entry.luau` drives the entry effects and the forward transition.
The editor-authored return trigger is in the second source. Thumbnails are exact
PNG copies under `project/thumbnails/`. `project/stage7_campaign.placesmap` is the
latest native Build+Play project; `campaign.placesmap` and `recovered-ui/` retain
the earlier explicit export/recovery comparison. Do not confuse the diagnostic
selected-level sibling package with complete project distribution.

The clean copied runtime is `/tmp/places-stage7-clean-install/`, with a real
copied `assets/` tree and no bundled levels or developer override. Native receipts
record each separate `/tmp/places-stage7-<label>-state` folder. `/tmp` is ephemeral;
the preserved local payload/fixtures and receipts remain the durable developer
handoff. There are no user deliverables exclusively in `target`.

## Next: separately authorized navigation implementation

Do not fold this into packaging. The user authorized reliable stacked-floor/ramp
connectivity and per-agent physical traversability; moving physics obstacles and
doors; reasonable local avoidance and actor-jam handling; stuck/unreachable
recovery and bounded replanning; and clean contextual editor coverage/route-debug
tools. Exercise furnished multi-floor traversal, blocked/open doors, moved objects
and unreachable goals in actual play, not only generated path data.

Current source facts (read-only audit, no navigation implementation in stage 7):

- `src/map_compiler/navigation.rs::bake` automatically rasterizes the authored
  `WalkableFloor` against compiled collision boxes and explicit obstacle/proxy
  inputs. Authors do not draw a whole movement mesh for each level. The default
  cell is 0.20 m. Floor regions, ramps and stairs use the shared sampled movement
  surface and step/slope neighbor rule.
- `src/engine/package/navigation.rs::NavGrid` / `NavClass` store PNV1 navigation
  in each compiled variant: one height/headroom/door-portal value per X/Z cell,
  per-class walkability bits and connected-region labels. This is a 2.5D grid,
  not a general layered polygon navmesh. Overlapping storeys cannot both be
  represented by that one cell height. Sampling can miss small/narrow passages.
- `src/map_compiler/mod.rs::build_navigation_record` always registers the reference
  humanoid plus distinct authored body profiles from placements/templates. Radius,
  height, step height and maximum slope control clearance; exact matching selects
  a runtime class. Current hard limits are eight classes, 2^21 cells and 256 door
  portals. Missing/invalid navigation is not rebuilt by the player.
- `src/engine/nav/query.rs::NavMesh::{nearest,path_into_bounded,segment_clear}`
  provides region-aware projection, bounded A* with a reusable binary heap/scratch,
  diagonal corner checks and topology-checked shortcut smoothing. Reachability is
  restricted to connected traversable cells and optional bounds; it is not
  permission to move anywhere. Path expansion has a hard 2^20 cap.
- Doors are baked as portals rather than fixed leaves. `engine::nav::NavDoorState`
  consults live closed/locked/open state and `can_open_doors`; AI requests doors
  and invalidates routes when their generation changes. This does not rebake a map.
- `src/engine/ai/movement.rs::AgentMove::step_with_leaves` substeps the shared disc
  collision rules against **static boxes and live door leaves**, follows the
  walking surface and refuses impossible steps. There is no general moving-body
  obstacle update, crowd avoidance or actor-jam solution in this input path.
  Replanning alone cannot solve a dynamic obstacle the graph does not represent.
- `src/engine/ai/mod.rs` owns idle/wander/follow/flee/investigate/pursue/catch state.
  `PERCEPTION_INTERVAL_S=0.20`, `REPLAN_TARGET_M=0.60`, `REPLAN_INTERVAL_S=0.50`,
  `PATH_RETRY_S=0.75`, `STUCK_SECONDS=0.55` bound existing behavior. These are
  heuristics, not a guarantee that every blocked actor recovers.
- `src/engine/ai/perception.rs` checks sight range/FOV and rays against static
  collision/live leaves, plus bounded typed hearing stimuli. It is decoupled from
  rendered pixels/audio. Predator targeting/last contact is not a full Source NPC
  tactical planner. The `ai` component uses a `nav_agent`; missing one warns and
  falls back to the reference body (`engine::entities::EntityWorld` registration).
  Pure fixed scenery and all arbitrary physics items do not acquire AI just because
  a package contains navigation; authored route motion remains its separate path.

The parent’s separate Valve research supplies the official Source comparison.
Do not infer Source-equivalent coverage or capabilities from the word “mesh”.

## Following queue

1. Author Closing Time in the **actual editor** with a real authoring-process
   video, using the currently integrated assets. Document every manual level-file
   workaround, not just the finished map.
2. Author the fleshed-out single-map horror mode **It Only Exists in the Beam**.
   Use the currently integrated assets for this initial mode. A creature is visible
   only when illuminated by the player’s flashlight; objects
   recovered among furniture unlock an achievable exit. Include explicit win and
   death feedback and usable respawn/reset. Perform three substantive passes:
   complete mechanics/objectives/win/death/reset; atmosphere/readability/pacing/fair
   creature behavior; then final gameplay/visual/performance polish. Each pass must
   actually verify win, death and reset and record findings/changes.
3. Both demos need practical future-improvements reports: concrete editor gaps
   requiring manual level/Luau edits, missing/unimplemented helpful sounds, and
   useful lighting/AI/physics/engine capabilities. Each item names its encounter or
   workflow, workaround and proposed improvement. Deduplicate shared items and
   distinguish intentional ordinary Luau authoring from a real editor limitation.
   Reporting is not permission to implement every suggestion.
4. After both game maps are made, integrate accepted finished 37 Home/shared
   models and rebuilt rubber duck,
   plus the new supply/janitor closet environment set when its separate cloud
   Blender/GIMP worker supplies verified archives/metadata. Reject the newer
   Spoonerman. Do no Mac modeling for the closet set. This stage did not fetch,
   modify or integrate those deliveries. No accepted new Home/shared archive was
   identified in the current `import/model-rework/` or Downloads listing; obtain
   the parent’s verified delivery identities before adoption. Current integration
   contracts are `docs/ASSET_SPECIFICATION.md`, `docs/MAP_AUTHORING_GUIDE.md`,
   `assets/completed-models.json`, and the existing ignored `import/model-rework/`
   review/delivery/unpacked folders. Preserve all mandatory fitted UV/PNG contracts.
5. Run the dedicated Luau/map-import safety review using **both finished modes**
   and malicious small fixtures. Reassess VM execution/memory/action budgets,
   scoped callback/reset lifetimes, in-process/cooperative limitations, archive
   paths/sizes/decompression, sources/images and publication failures in release
   binaries. Preserve the release foreign-unwind checks. Stage 7’s retained
   `malformed/` fixtures and refusal logs are a starting point, not a finished-mode
   safety certification.

Stop before stages 8/9 (future bundled menu level and broad distribution).
Backlog #25 and #26 remain outside this queue. Broader #18/#21 acceptance remains
open, including native Windows/Linux and final installation/distribution signoff.

## Test policy carried forward

The user wants small, cheap, high-value regression/contract tests, not zero tests.
Do not purge the suite. Avoid redundant coverage, brittle implementation-detail
checks and expensive fixtures without demonstrated value. Explain each addition’s
protected behavior, unique regression value and proportional cost; keep tests in
the owning engine/game/editor/map_compiler tests folder and wire discovery.
The [stage 7 test audit](editor-0.12.0-task7.md#rust-test-count-audit) distinguishes
67 inherited baseline tests, per-stage additions, four new stage 7 cases and
separate production/native script checks. Ordinary CI must not bake full maps.
