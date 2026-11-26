# Places: remaining work

**Queue status.** Work on `0.14.0-dev`, on items Connor has approved. Sync to the current branch and check issue status before starting. This file tracks remaining work only; completed entries are preserved in Git history. Item numbers stay stable across cleanups. Keep deferred items deferred. Finding numbers refer to the internal review.

## Harness instructions

1. **Implement with Sol 6.2 High by default.** Use Extra High only when needed. Use a fresh Codex context for each item, with its scope, dependencies, known evidence and acceptance checks. Hand over narrow diffs and exact paths. Keep one source writer at a time and one canonical checkout per machine. Use independent agents only where work does not overlap.
2. **After every item, run a separate Sol 6.2 Medium code audit.** Use a fresh reviewer context and the exact diff or commit. Review correctness, idiomatic Rust, regressions and whether any new tests protect current behavior. For research, documentation or map-only items, review the actual deliverable and confirm there were no unintended code changes; do not invent a test suite. Add actionable findings to the **next runnable queue item** with the affected paths and the required check. If a finding blocks acceptance, keep the affected issue open until fixed and verified. If there is no next item, append a short follow-up.
3. **Mark off completed work on GitHub.** After focused validation and independent review, commit and verify the push to `0.14.0-dev`. Check off the queue item in this file, update the corresponding GitHub issue checklist and close the issue as completed only when its applicable scope is finished. Do not close a multi-item issue from a partial pass or treat an unverified fix as complete. For items without a linked issue, the GitHub queue file is the record; do not create a ticket just to close it.
4. **Purge task temp files immediately after closeout, including failed or canceled runs.** Remove disposable bakes, caches, staging directories, logs and generated scratch files from the machine where they were made. Run `cargo clean` on the exact task-owned, idle target/build directories once deliverables are out of them. No archives, duplicate caches or recovery indexes for reproducible output. Preserve source work, requested deliverables and only the exact inputs an unfinished dependent item still needs; purge those temporary inputs after that dependency is done.
5. **Avoid rework.** Prefer compact editor-authored fixtures and narrow checks. Use the direct executable, not an `.app`, for Mac playtests. Author maps manually in the editor; the built-in compiler may export editor-saved maps. Keep required large bakes together behind their compiler fixes; move only a blocked large-map check into that pass, not the independent code work. If a reported bug cannot be reproduced after a bounded check, record that outcome and keep moving. Do not turn full Windows/Linux validation into a gate for this queue. No force pushes, merges to main, releases, routine writeups or new reference documents unless explicitly requested.

## Release blockers and compact checks

### 2. Published gameplay changes: outstanding native witnesses
**Open for scripted native gameplay verification only.** The published NPC changes and prop release passed focused tests; native Mac checks covered cup pickup/release, stalled pursuit recovery, and ordinary two-floor routing.

**Additional reviewed Mac evidence:** the native pursuer cleared a stalled route and issued a retry while the obstruction remained. Ordinary AI reached matching horizontal goals on two different floors, but this did not exercise scripted floor selection. A native fixture for a blocked scripted move that stays stopped until explicitly reissued is still needed.

### 10. [GitHub #53: Reproduce and fix the Long Way Home FPS drops](https://github.com/csd113/Places/issues/53)
**Fixed the reproduced Danger second-exit CPU hitch on the Razer. #53 remains open for broader Long Way Home
acceptance.** The [published fix](https://github.com/csd113/Places/commit/8c3e216d884a57c1f4d75c47f7457df78ef25eb0)
is the exact independently reviewed Sol 6.2 Medium diff, originally `8a4cc624` (audited patch SHA-256
`55814dc3c2fa53e213fdf6a5556822e4554f666191c9560e76a3c1e374cde2fb`).

The same verified Long Way Home archive and first-exit, re-entry, second-exit route ran on the bare release player
with the RTX 3070 Laptop GPU, actual DX12 Immediate, a physical 1280×720 drawable, V-sync off and no frame limiter.
Two complete baseline/fixed replays measured second-exit FPS of 84.3/89.4 → 125.7/124.2, CPU update p95 of
21.78/20.97 → 7.00/7.17 ms and all-frame p99 of 49.75/49.09 → 40.64/39.32 ms. In the nearby control pair,
first-exit frame p95 was 11.50 → 11.56 ms (172.2 → 187.5 FPS). On the second exit it was 24.53 → 10.90 ms
(78.2 → 123.0 FPS), with all-frame p99 54.04 → 39.81 ms and CPU update p95 21.81 → 6.99 ms.
Across all three fixed replays, p99 was 13.36–14.00 ms when excluding only scripted input-trace frames and their
immediate successors. The unfiltered spikes are recorded above, and the active second exit remains slower.

The Tall Guest was rebuilding the base pose and inspection blend for all 17 aim weights on each safe step.
The fix scores the same weights in the same order using the acoustic socket chain and leaves the full selected
pose, contacts, collisions and step budgets unchanged. Instrumented aim p95 fell from 12.02–13.37 to
0.41–0.42 ms. Clearance still costs about 2.7 ms p95 in the instrumented second exit. Strict workspace Clippy,
the full locked all-features release build and the exact socket-equivalence regression passed. The workspace
suite passed 735/736, with the same mannequin carry/reset assertion failing on the unmodified published base.
No shared bake or whole-level performance acceptance was run.

**Fixed the separate periodic support-journal stall.** In two alternating trace-off Razer DX12 Immediate A/B
pairs on stock `706dd2d8` versus the reviewed patch, peak `loop_ms` in the estimated ±150 ms Danger journal
window fell from 31.14/30.75 to 7.11/7.24 ms. The compact records parsed at 622/623 bytes instead of truncating
at 4,096. A separate explicit trace recorded 13,039 ready presents with none missed and retained full periodic
and state-change snapshots. Strict Clippy and the locked all-features release build passed. This covers
representative Long Way Home scenes, not the full level. It did not address the separate Introduction spike.

**Reduced the Introduction spike.** Reusing exact obstacle sweeps within each Feltling query reduced the first
two measured Razer DX12 Immediate loop intervals from 30.40/29.96 ms to 26.24/26.46 ms. All three same-snapshot
comparisons matched the stock route results exactly. The spike remains, other scenes were not tested for this
change, and item 10 and #53 remain open.

## Audio, HUD and environment work

### 23. [GitHub #34: Optional HUD, health and melee](https://github.com/csd113/Places/issues/34)
Keep the existing six-HP player, three hearts, disabled-by-default HUD and crowbar behavior. Finish human NPC defaults (six HP, honoring lower authored values and immortality), monster defaults (generally at least 18 HP with design-specific balancing), and the flashlight indicator (visible only on an enabled map while on, no off-state placeholder). Verify repeated transitions/reloads/toggles without stale HUD state. Keep HUD visibility independent of gameplay enablement.

**Implemented. Independent Sol Medium code review and compact Mac native validation passed.** Two rooms hand-authored in the native editor were exported together and imported into an isolated player profile. At six HP the visible HUD showed three full hearts, and one HP of damage emptied one half. The other room still accepted damage with the HUD hidden. The flashlight beam toggled in both rooms, and the indicator appeared only in the visible HUD room while lit. Two round trips through authored transitions and a same-map reload applied the destination rules with no stale indicator in the loaded room. Death and developer respawn showed zero and then six HP while the lit beam and indicator remained in sync. The imported NPCs reported 6/6 default HP, an authored 2/3 HP, and explicit immortality unchanged after damage.

**Native encounter acceptance remains open.** The focused Rust test passed for the 18 HP unconfigured encounter zombie and the authored four-HP exception. No suitable non-Backrooms encounter package is available, and the editor does not author the top-level encounter mode. Leave item 23 and #34 open for that check.

## Research only, then multiplayer

### 30. [GitHub #15: Multiplayer and in-session text chat](https://github.com/csd113/Places/issues/15)
**Deferred.** Leave GitHub #15 open. Keep this large feature behind core fixes and smaller work. Define a bounded join/host/leave and reconnect flow, synchronize movement and visible players, choose and document the shared interactions and world state, and add in-session text chat. Validate two players together and cleanup/reconnect under latency or disconnection. Do not broaden into unrelated engine or account work.

## Documentation and standalone showcase

### 32. [GitHub #43: Technical architecture and API documentation](https://github.com/csd113/Places/issues/43)
**Deferred.** Leave GitHub #43 open. The issue explicitly requests documentation, but the newer standing direction is **no routine task reports or reference sheets**. Reconcile that before writing. Cover current engine/game/editor/compiler responsibilities, shared ownership and important data contracts, and the real editor → compiler → package → player path. Verify links/examples against current code and prefer concise maintained content to copying the issue’s historical multi-document scope verbatim. This isn’t an architecture rewrite.

### 34. Christmas environment props and models
**Queued.** Create all-new, Christmas-specific models and textures in the Dot’s own Blender and GIMP, not on Connor’s computers. Do not reuse, recolor, or repackage existing Arcade gifts or Winter string lights. Match Places’ low-poly style, in-game scale, and asset conventions.

- Hanging Christmas wreath
- Wrapped Christmas presents
- Candy cane
- Three distinct small wooden toys
- Small angel statue
- Small nutcracker
- German marzipan loaf
- Christmas lights
- Small snowman model
- Gingerbread man cookie
- Gingerbread house that stays intact while held and breaks into separate pieces on impact when dropped. Verify pickup, drop, and breakup in the editor playtest.
- Lit Christmas candles with visible flames
- Wall-mounted snowflake decoration
- Snow globes with animated snowfall visible inside the glass when seen up close. Keep the snow within the globe and skip the effect at a distance.
- Placeable Santa Claus NPC
- Optional little elf costumes for Feltlings, selectable per Feltling without replacing their default appearance
- Wall-mounted Christmas stocking decoration
- Small pinecone props

This item covers new assets and their editor-catalog validation. No new map, Mall decoration, or full bake is in scope.

## Not in this queue

- GitHub #27–#31 are already closed as completed. Do not reopen, replay their old checklists or restore their work as separate tasks. Only overlapping checks independently required by an active issue still apply.
- GitHub #44 and #36 are closed as not planned. The separate Windows RTX study (#57), source-split recommendations (#56), and other unselected future features remain out of scope.
- Published PocketCHIP and zombie texture work, the completed FPS/compiler rounds, test-pruning recommendations and prior Mac/Razer cleanup are not new tasks. GitHub #50/#51 are furniture issues, not the internal NPC findings with the same numbers. Nothing here starts or resumes a run.
