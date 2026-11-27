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

### Research: Local agent API for the Places level editor
**Next priority. Research only; Mall work is paused.** Trace the current editor's commands, geometry validation, undo/redo, and save path. Compare a local CLI, local IPC, and an optional MCP adapter. Recommend the smallest cross-platform design with exact integration points, costs, risks, and a staged rollout.

- **Full editor parity in both directions.** Inventory every Basic and Advanced workflow, including save, export, bake, and Quick Test. Require the same command implementations, selection and mode prerequisites, validation, intentional overrides, and confirmations as the editor UI. Verify no feature is missing and no API action or batch can do something the editor cannot. Plan an automated parity check for future editor features.
- **Safe, efficient commands.** Specify versioned typed commands, stable object IDs, focused queries and deltas, actionable validation errors, revision-bound preview/apply, atomic reversible batches, one-step undo, retry receipts and safe recovery. Evaluate human edits during preview, cancellation, lost replies and repeat requests. Keep local access opt-in and scoped. No direct level-file writes, arbitrary scripts, or unrestricted filesystem access. Save, export and bake remain explicit editor actions, not hidden batch side effects.
- **Proof and benchmark.** Plan disposable editor-authored examples for railings, two-landing escalators, and shop lights. Benchmark the proposed API against current Codex Computer Use with the same model, starting map, visible viewport, and acceptance checks. Count full-task model tokens, including tool schemas, responses, images and retries, plus calls and elapsed time. Recommend a threshold for a meaningful, repeatable token reduction with equivalent results and no loss of oversight. If counters are unavailable, label estimates and measurement gaps. Include visual or Quick Test readback, invalid placement and stale preview rejection without mutations, and an exact undo check. Identify gaps before recommending implementation. Do not implement, edit production maps, or resume the Mall in this item.
- **Visible live viewport required.** The agent may only work through the running, monitorable editor with the viewport open. No headless mode or fallback writer. Audit visible activity, human pause/takeover, and what happens if the window is hidden, closed, or switches projects during a job.
- **Automatic gap reports.** Distinguish API parity bugs and genuinely missing editor features from normal validation, permission, or transient errors. Keep reports local, actionable, and deduplicated. Do not file or queue them externally without review.

### 30. [GitHub #15: Multiplayer and in-session text chat](https://github.com/csd113/Places/issues/15)
**Deferred.** Leave GitHub #15 open. Keep this large feature behind core fixes and smaller work. Define a bounded join/host/leave and reconnect flow, synchronize movement and visible players, choose and document the shared interactions and world state, and add in-session text chat. Validate two players together and cleanup/reconnect under latency or disconnection. Do not broaden into unrelated engine or account work.

## Documentation and standalone showcase

### 32. [GitHub #43: Technical architecture and API documentation](https://github.com/csd113/Places/issues/43)
**Deferred.** Leave GitHub #43 open. The issue explicitly requests documentation, but the newer standing direction is **no routine task reports or reference sheets**. Reconcile that before writing. Cover current engine/game/editor/compiler responsibilities, shared ownership and important data contracts, and the real editor → compiler → package → player path. Verify links/examples against current code and prefer concise maintained content to copying the issue’s historical multi-document scope verbatim. This isn’t an architecture rewrite.

## Not in this queue

- GitHub #27–#31 are already closed as completed. Do not reopen, replay their old checklists or restore their work as separate tasks. Only overlapping checks independently required by an active issue still apply.
- GitHub #44 and #36 are closed as not planned. The separate Windows RTX study (#57), source-split recommendations (#56), and other unselected future features remain out of scope.
- Published PocketCHIP and zombie texture work, the completed FPS/compiler rounds, test-pruning recommendations and prior Mac/Razer cleanup are not new tasks. GitHub #50/#51 are furniture issues, not the internal NPC findings with the same numbers. Nothing here starts or resumes a run.
