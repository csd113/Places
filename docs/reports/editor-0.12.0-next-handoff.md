# After navigation: authored modes, assets and safety review

Stay on `0.12.0-staging`. No main merge, release, branch deletion, `.app`
packaging/launch or roadmap stage 8/9 work is authorized by this handoff.
The navigation pass stops here; the next task is Closing Time.

## Current handoff and runnable tools

Repository: `/Users/connordawkins/Documents/GitHub/Places`.
Navigation implementation, compatibility, tests and native results are recorded
in `docs/reports/editor-0.12.0-navigation.md`; the exact local receipt is
`local/verification/0.12.0-navigation/navigation-handoff.json`.

Preserved direct editor/compiler/player are under
`local/verification/0.12.0-navigation/payload/`. `try-editor.sh` opens the compact
navigation authoring project with repository assets and isolated manual state.
`try-player.sh` imports and opens its compiled fixture. `replay-native.sh layers`
assigns deterministic diagnostic goals through the actual planner/mover and
records another native demonstration in a new directory. Other explicit cases
are clearance, door, obstacle, meeting, unreachable and performance. For timing,
verify the current display refresh and pass `--display-refresh-hz HZ` rather than
assuming the previous display's setting. SDL3 is supplied by the host.

The fixture is acceptance tooling, not either requested game mode. It uses only
currently integrated assets. Baked navigation now retains stacked storeys and
per-body collision-probed links; physics/door state and local actor passing are
runtime checks, without global rebaking. Editor `Navigation >` exposes an
explicit current-source bake, body/instance selection, floor slices, coverage
and reachable/invalid/unreachable route feedback. Verify live props/crowds in
Play. Exact behavior and remaining limits are in the navigation report.

Navigation alone is PNV2/record 2. Older PNV1 playback is refused by name and
requires rebuilding editable source with the new compiler. Complete Stage 7
archives can still recover their exact editable files. Source/project/package
container and lighting versions remain unchanged. Stage 7's original handoff,
payloads, campaign and evidence remain under
`local/verification/0.12.0-task7/`; do not delete or overwrite them. Its report
is `docs/reports/editor-0.12.0-task7.md`. Keep all useful evidence and runnable
binaries outside `target`, and check for other active work before cleaning it.

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
