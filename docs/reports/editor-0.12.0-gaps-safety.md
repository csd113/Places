# Closing Time / Beam implementation and safety pass

Scope: `0.12.0-staging`, starting at `f6090911`. Accepted gameplay, game Luau,
R2 mannequin, Spoonerman and unrelated assets are preserved. Closing Time alone
adds `requires_full_lighting: true`. No main merge, release or later queue stage
belongs here. Final receipts and runnable delivery live in
`local/gaps-safety-0.12.0/`, outside `target`.

## Recorded gap checklist

The checklist deduplicates the actual [Closing Time](editor-0.12.0-closing-time.md)
and [Beam](editor-0.12.0-beam.md) handoffs; ordinary Luau gameplay remains intentional.

| Recorded gap | Implemented behavior |
| --- | --- |
| Missing instance/state/event/condition controls; trigger-only script links | Focused Capabilities hub, typed state, exact selected event/actions/conditions, prop/door/trigger script links; structured-reference rename before scripting, with undo/save/reopen validation. |
| Exact item and unheld cannot compose; only first event editable | Independent identity/held policy and multi-event editing preserve sibling bindings. Optional centre/settled-centre receipt uses bounded physical paths and actual sleep. `item-receipts-v1` prevents silent older-host fallback. |
| Missing circuits, attached sources, detailed AI/beam/audio/reset settings | Separate paged forms expose existing capabilities and validators. Fitted installed fixed shapes retain their real compound; player standable dimensions stay independent. |
| Invalid intermediate transforms and failed Add editing the old selection | Complete-pose draft/validation/preview commits once; refused Add clears selection. Find filters the scene; Y uses its explicit authored basis. |
| Carrying forces drop before door use | E uses an exactly aimed, reachable, unobstructed unlocked door while carrying; looking away or aiming at a locked door retains safe drop and existing door collision safeguards. |
| Automatic start/update/hint reset loses shared lifecycle | Propagate dispatch accounting/reset, stop later callbacks/events and perform the common player/world/item reset. |
| Static holds Closing's independent fixed-prop circuits | Explicit Full requirement, actual prepared atlas admission and `full-lighting-v1`; lesser-mode requests are refused. No runtime bake or invented Static combinations. |
| Silent unavailable labels and missing sound preview | Label targets require actual interactable capability, including disabled controls; bounded existing message/outcome feedback remains. Listen previews one preloaded cue quietly for at most three seconds. |
| Resource-root audio admission; project verify; repeated semantic-only bake | Configure the explicit root before semantic admission; select project record views for provenance; seed exports only from verified prepared records/signatures and retain atomic publication. |
| Floor-contact noise, door/storey and pair-budget diagnostics | Separate supported opaque bases from thin/alpha/duplicate surfaces, spatially prioritize pairs, include authored door colliders and preserve storey ownership; incomplete coverage is explicit. |

Controls use small owning modules and paged sections rather than adding every
advanced property to placement. Raw unknown fields survive. Readability/crosshair
assets and the accepted game's minimal HUD remain unchanged.

## Safety boundaries

Luau retains its closed host surface, frozen libraries, no OS/network/module
loader/FFI, 2 MiB per VM and existing source/module/action/reference limits.
Every host invocation now has a 256-call native-query/action budget. Execution
deadlines begin after host setup/compilation, while host setup still counts
against frame accounting. Failed calls publish no staged effects and disable
the module until a fresh run.

An actual sparse `table.move` probe exposed native work beyond cooperative VM
interrupts. Its wrapper preserves ordinary overlapping/negative/optional-target
semantics, accepts real integral 32-bit indices, and caps work at 8192 elements.
The retained before receipt measures a whole compiler process, not isolated
library latency. Release probes include the count-based refusal.

Archive admission checks central-directory framing, encrypted/control/duplicate
entries, existing path/count/aggregate/decompression limits and hash ownership.
Recovery validates one bounded owned snapshot; temporary directories are reserved
atomically and publication never replaces another folder. Source/output identity
is checked after compression at publication. Special files/FIFOs are refused.
Installed dependencies stream through 8 KiB buffers under the actual PNG/GLB
encoded limits, stay inside the canonical asset root, and use regular opened
handles. Repeated portable dependency identities are refused before hashing.
Independent focused review checked these boundaries and the reset integration.

These are in-process cooperative safeguards, not a process sandbox or hard
real-time/RSS guarantee. Native-library calls/decompression can run between
interrupt/cancellation checks. A huge otherwise valid installed resource set
still costs bounded file hashing; malicious concurrent filesystem mutation is
not a promised local authoring sandbox. Saturated event queues can lose a wholly
outside-to-outside transient crossing; final exits are retained. Neither finished
game approaches that saturation.

## Verification and delivery

Required strict Clippy, locked all-feature production build, formatting and
owning-component regressions are retained in `checks/`. The small tests protect
specific capability, transaction, native-cost, reset, receipt, door and geometry
failures; no routine test or CI check bakes a shipped map. The source Python suite
passes 369 checks with 16 supported skips on this local CPU run; separate native
acceptance covers the actual macOS/Metal player/editor.

Final native outcomes, exact-head CI, payload hashes, Library identity, source
custody, cleanup and preserved-binary smoke receipts are recorded in `handoff.json`.
The report is paired with that machine-readable receipt rather than embedding a
self-referential commit hash. Use its runnable commands and final package paths.

Native component/event/save/reopen acceptance passes 36 assertions; additional
pose-preview/one-undo/refused-placement/audio acceptance passes 13. Actual SDL
focus is recorded at every checkpoint. The original Beam win route exposed a
carried-item deposit regression when the locked exit behind the counter took
interaction priority. The target rule and existing small regression now preserve
set-down for locked doors. The failed native attempt is retained alongside the
final rerun rather than counted as acceptance.

The final native player passes eight campaigns (53 focused captures, 202 audit
checks, plus the explicit post-win reset checks): both ordinary win routes;
Closing's alternate return/switch order, held removal and restart; Beam's two
death/reset cycles, return reentry/removal, hints/sign reset and beam/partition
occlusion; carried-door opening/traversal/set-down; and Closing's actual Full
retention, quality changes, circuit state and reset. A closer held-door approach
also retained its physical obstruction guard. These are correctness runs; no
uncapped-throughput conclusion is drawn from their capture timing.

The final delivery compiler and both final packages pass all 44 package-safety
checks and all 11 hostile Luau initializer refusals. A name-only export of an
isolated Closing copy hits the verified prepared-world cache, preserving all
prepared record references and lighting fingerprint while updating package
provenance. The delivered Closing package passes project-aware
`verify --require-current`. Beam's accepted package remains byte-identical.

Read-only native Safety scans preserve both sources. Closing has 29 entries
(two notes, 27 review candidates); Beam has ten (two notes, eight review
candidates, including sampled leak/drop/sliver findings). These remain contextual
review candidates under the documented sampled-geometry limits, not a claim that
either map is proven closed. No automatic correction or ignore was applied.

## Explicit remaining work

Arbitrary mixed-circuit Static preparation requires a renderer/prepared-record
feature. Connected-storey topology/room ownership, project/map ID migration,
arbitrary collider construction/part overlays, additional sound production and
polish, raw-SDL app inventory integration, and later editor/distribution stages
remain separate. Scripted ID/state rename deliberately refuses opaque Luau
rewrites. Settled-centre receipt does not identify a particular supporting shelf.
Safety remains sampled, uses authored door poses/static models, and does not
prove mesh closure or improve probe-grid sampling. No Windows/Linux native
certification or new FPS comparison is claimed. Broad issues remain open.
