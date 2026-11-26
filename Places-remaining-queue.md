# Places: remaining work

**Queue status.** Work on `0.14.0-dev`, on items Connor has approved. Sync to the current branch and check issue status before starting. This file tracks remaining work only; completed entries are preserved in Git history. Item numbers stay stable across cleanups. Keep deferred items deferred. Finding numbers refer to the internal review.

## Harness instructions

1. **Implement with Sol 6.2 High by default.** Use Extra High only when needed. Use a fresh Codex context for each item, with its scope, dependencies, known evidence and acceptance checks. Hand over narrow diffs and exact paths. Keep one source writer at a time and one canonical checkout per machine. Use independent agents only where work does not overlap.
2. **After every item, run a separate Sol 6.2 Medium code audit.** Use a fresh reviewer context and the exact diff or commit. Review correctness, idiomatic Rust, regressions and whether any new tests protect current behavior. For research, documentation or map-only items, review the actual deliverable and confirm there were no unintended code changes; do not invent a test suite. Add actionable findings to the **next runnable queue item** with the affected paths and the required check. If a finding blocks acceptance, keep the affected issue open until fixed and verified. If there is no next item, append a short follow-up.
3. **Mark off completed work on GitHub.** After focused validation and independent review, commit and verify the push to `0.14.0-dev`. Check off the queue item in this file, update the corresponding GitHub issue checklist and close the issue as completed only when its applicable scope is finished. Do not close a multi-item issue from a partial pass or treat an unverified fix as complete. For items without a linked issue, the GitHub queue file is the record; do not create a ticket just to close it.
4. **Purge task temp files immediately after closeout, including failed or canceled runs.** Remove disposable bakes, caches, staging directories, logs and generated scratch files from the machine where they were made. Run `cargo clean` on the exact task-owned, idle target/build directories once deliverables are out of them. No archives, duplicate caches or recovery indexes for reproducible output. Preserve source work, requested deliverables and only the exact inputs an unfinished dependent item still needs; purge those temporary inputs after that dependency is done.
5. **Avoid rework.** Prefer compact editor-authored fixtures and narrow checks. Use the direct executable, not an `.app`, for Mac playtests. Author maps manually in the editor; the built-in compiler may export editor-saved maps. Keep required large bakes together behind their compiler fixes; move only a blocked large-map check into that pass, not the independent code work. If a reported bug cannot be reproduced after a bounded check, record that outcome and keep moving. Do not turn full Windows/Linux validation into a gate for this queue. No force pushes, merges to main, releases, routine writeups or new reference documents unless explicitly requested.

## Release blockers and compact checks

## Audio, HUD and environment work

## Research only, then multiplayer

### 30. [GitHub #15: Multiplayer and in-session text chat](https://github.com/csd113/Places/issues/15)
**Deferred.** Leave GitHub #15 open. Keep this large feature behind core fixes and smaller work. Define a bounded join/host/leave and reconnect flow, synchronize movement and visible players, choose and document the shared interactions and world state, and add in-session text chat. Validate two players together and cleanup/reconnect under latency or disconnection. Do not broaden into unrelated engine or account work.

## Documentation and standalone showcase

### 32. [GitHub #43: Technical architecture and API documentation](https://github.com/csd113/Places/issues/43)
**Deferred.** Leave GitHub #43 open. The issue explicitly requests documentation, but the newer standing direction is **no routine task reports or reference sheets**. Reconcile that before writing. Cover current engine/game/editor/compiler responsibilities, shared ownership and important data contracts, and the real editor → compiler → package → player path. Verify links/examples against current code and prefer concise maintained content to copying the issue’s historical multi-document scope verbatim. This isn’t an architecture rewrite.

## Compiler and VR follow-ups

### 36. Find ways to speed up map compilation and lighting bakes
**Queued.** Measure matched exports of compact and representative editor-authored maps. Compare forced Full builds with unchanged and incremental exports, holding quality and worker counts stable. Break down lighting solve, other compile phases, caching, and packaging; verify output and prioritize the strongest measured opportunities without lowering quality. Do not reuse a cached stage as evidence of faster Full solving or reopen the earlier RTX scope.

### 37. Investigate Valve Index compatibility
**Queued, feasibility only.** After item 36, assess Windows SteamVR/OpenXR support on the Razer with the Valve Index. Check headset connection and tracking prerequisites, graphics adapter/runtime compatibility, and what the existing renderer, input, and frame timing would need. Separate source findings from anything verified on hardware, and recommend the smallest viable prototype. Do not implement VR under this item.

## Editor follow-ups

### 38. Fix the eight Basic mode usability findings
**Queued.** Follow up on the completed Basic mode audit. Keep the existing working workflows and prioritize simple, guided controls in Basic. Keep raw records and power-user controls in Advanced. No unrelated redesign.

1. Fix the Materials layout so search results and tiles never cover **Add Escalator** or **Place Railing**. Verify both controls are visible and clickable at the 1440 × 816 audit viewport.
2. Make browsing or selecting a prop tile choose what to place next, not silently replace an already selected prop. With a Chair selected, browse Armchair and cancel without changing the Chair or undo history; place a separate Armchair and confirm Undo removes only that placement. Preserve explicit replacement in Properties and check the electrical-panel and escalator tile paths.
3. Give Basic stairs a valid pair of distinct floors when available, or an **Add Upper Floor** shortcut. Make insufficient run and landing clearance actionable without weakening the existing check. Verify a valid pair places stairs and the cramped starter layout still refuses invalid placement.
4. For escalators, make the first click visibly mark the lower landing and prompt for the higher landing instead of showing a premature same-floor error. Verify valid placement and clear feedback for invalid endpoints.
5. Make Basic **Room** start with the existing adjacent-room preview and apply flow rather than the raw coordinate editor. Keep record editing in Advanced.
6. Make railing wording match what it does: clicks add draft points, Cancel describes its actual effect, and committing a railing is one undoable action. Preserve default floor attachment.
7. When there is no wall for a door, show **Add a wall first** or equivalent actionable guidance. Verify ordinary placement and Undo still work.
8. Label UV material tiles as applying to the selected surface, not choosing what to place next. Verify the existing default placement flow still works.

Use a compact editor-authored fixture and native Basic mode checks. Confirm Quick Test still preserves the camera, unsaved edits, and undo history. Have a separate reviewer verify the exact diff and observed behavior.

## Not in this queue

- GitHub #27–#31 are already closed as completed. Do not reopen, replay their old checklists or restore their work as separate tasks. Only overlapping checks independently required by an active issue still apply.
- GitHub #44 and #36 are closed as not planned. The separate Windows RTX study (#57), source-split recommendations (#56), and other unselected future features remain out of scope.
- Published PocketCHIP and zombie texture work, the completed FPS/compiler rounds, test-pruning recommendations and prior Mac/Razer cleanup are not new tasks. GitHub #50/#51 are furniture issues, not the internal NPC findings with the same numbers. Nothing here starts or resumes a run.
