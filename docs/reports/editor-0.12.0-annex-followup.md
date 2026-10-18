# Relay Annex editor follow-up

Work on `0.12.0-staging` from `888e6118`. Accepted campaign sources are inputs;
verification works on an isolated project copy under `local/editor-annex-0.12.0`.
No environment hub, distribution rollout, audio stage, merge or release is included.

## Implementation and verification checklist

| Gap | Implementation target | Verification target | Status |
| --- | --- | --- | --- |
| R1 | Complete ramp creation draft before admission | Invalid initial camera target; valid complete pose; cancel/undo | Implemented; native verification below |
| R2 | Staircase and guardrail forms; existing room landing tools | Create/edit, physical connection, save/reopen | Implemented; native verification below |
| R3 | Selected-floor local overrides with explicit ownership | Stacked floors retain independent materials/heights | Implemented; native verification below |
| R4 | Height-aware legacy route support and authoring | Upper floor move/blocked edge; route preview | Implemented; native verification below |
| R5 | Physical item contact and release diagnostics | Dormant contact and blocked held movement | Implemented; native verification below |
| R6 | Water forms and vertical slice | Basin and exit ramp coverage; save/reopen/play | Implemented; native verification below |
| R7 | Outdoor ceiling, sky, weather, directional source forms | Intake environment preview and roundtrip | Implemented; native verification below |
| R8 | Item centre/sleep/receipt debugging | Actual live receiver membership/state | Implemented; native verification below |
| R9 | Preserve selected-map cache manifest check | Tower graphics cache hit and circuits | Inherited fix retained; native quality replay below |
| R10 | Structural-plane/shaft diagnostic classification | Tower candidates remain reviewable; no blanket ignores | Implemented; native verification below |
| R11 | Bounded explicit per-map display metadata | Friendly inactive titles without loading world semantics | Implemented; native verification below |
| R12 | Guidance wrapping and authoring preview | Long prose, narrow windows, keyboard focus | Implemented; native verification below |

Operation measurements cover open/switch, scene selection/search, transform,
inspector edits, history, safety scan/fix, save/reopen and build/export progress.
CPU document timings and native handler/preview timings are distinct. No paced
native capture is used as uncapped throughput evidence.

## Current implementation

The grouped Space and environment sheet edits complete existing engine records.
Ramp creation enters a draft before admission. Staircase/rail forms work alongside
the existing owned room/landing tools. Selected-floor overrides split only the
chosen room, retain the ceiling plane, preserve unknown source fields and add
short boundary faces for a height change. Existing ambiguous global patches,
regions and intersecting slopes are refused explicitly; their legacy semantics
remain intact. Explicit GLOBAL patch/region forms expose that effect and allow
removal before creating the local override. A native probe refused the ambiguous
override, deleted the patch through its form, applied the local section and
restored all original geometry with undo. Every Apply uses the same validated
undo/source path.

Legacy routes sample support near the actor's current feet in both admission and
runtime movement. They cannot transfer silently to a lower storey. Their paged
forms retain ordered steps and show a sampled path preview. AI and route ownership
remain exclusive; AI-only beam response must be removed explicitly before choosing
no AI. The native refusal attempts are retained, followed by a successful UI-authored
9.6 m route and actual 5.00→5.48→5.29 m movement at that height.

Safety measures real dormant rigid-shape penetration against item-world scenery,
including explicit fixed compounds. It excludes resting contacts within 2 mm and
offers no automatic item relocation. A 5 cm native placement probe reports exactly
0.0500 m penetration, then undo restores it. Play warns after a blocked release or
sustained held-motion refusal. The opt-in `/trigger show` overlay draws actual item
centres/states and receiver bands under the existing filter/receipt policy. Green
means eligible occupancy, separately from script conditions and completion.

Water previews include X/Z elevation slices through actual room/ramp/stair support.
Exterior forms expose open ceilings, installed sky references, snow/weather and
directional illuminators. Sky/weather/light previews temporarily use authored
lighting; cancel restores Inspect. Native screenshot review caught and corrected a
last-row/navigation overlap. Retina/antialiased text, existing icons and crosshair
are retained; no visual asset or catalog file changed.

201 measured opposite floor/ceiling pairs in Tower are now explicit
`shared-storey-plane` notes. Fourteen perimeter and three single-height flood
candidates remain visible for manual connected-shaft review. Other reversed/model
candidates also remain; no blanket ignore or topology repair was introduced.
Placement-ignore metadata and the existing automatic-correction protection gate
remain unchanged.

Per-map `display_name` is bounded explicit project metadata. Inactive menu titles
read the project index and retain ID fallback, without reading inactive semantics.
The existing corrupt-inactive-payload regression now also checks this title.
Guidance wraps words/long tokens into six-row pages under the unchanged six-line,
72-character scripting limit. Page Up/Page Down browse; Return/Escape dismiss.
Native Beam exposed the first pagination version consuming Return; the corrected
key policy has a focused regression and the failed native attempt is retained.

## Performance evidence

`editor-profile` is an explicit operation probe, outside automatic tests and CI
map baking. On the unchanged three-map source, open averages 3.24 ms before, 3.27 ms in the first after sample and 2.01 ms
in the final warm repeat. Transform+undo is 2.69→2.71 ms and redo+undo
2.60→3.00 ms in the final repeat. Filesystem/CPU timing variance limits these samples. Cached object indexing removes
per-frame enumeration/allocation, and dirty-state comparison moves to accepted
edits/history changes. The CPU dirty check falls from about 18.7 microseconds to
a cached Boolean read. These small measurements do not imply a game FPS gain.

Matched native recipes perform three pose/undo/redo cycles, inspector editing,
15 map switches, safety and save. Selection handler averages 0.163→0.098 ms;
search 0.117→0.073 ms. Preview refresh remains about 200–250 ms. Tower switching
is 218→229 ms and Intake 219→251 ms with current-scene model retention; no overall
latency speedup is claimed. The single coalesced preview worker keeps one active
and one latest pending request. Optional texture retention is capped and model
retention follows current scene dependencies. Peak RSS is 430.8→430.5 MiB in these
repeated flows; this is not a meaningful measured memory reduction.

The edited three-map native export takes 90.63 s after cancellation. A subsequent
warm final Build+Play/return cycle takes 9.16 s including 6.5 s of actual player actions.
The UI remains responsive, reports progress, imports the complete project and
reaps the player before returning to a focused editor. Native latency runs use
ordinary paced editor presentation and are distinct from uncapped FPS evidence.

Final native preview wait averages (before → after, ms) are open 402→410,
transform preview 212→212, transform apply 196→193, inspector 206→203,
undo 202→200, redo 201→199, Discharge switch 181→200, and Safety 150→167.
Save handler time is 3.85→3.75 ms. A matched deliberate-duplicate correction
probe has mean preview waits 218.0→218.6 ms and apply 172.6→173.6 ms;
preview/cancel/apply/undo preserve exact source. These are operation samples,
not statistical latency guarantees. Final compiler identity forced a cold
Build+Play rebuild (93.58 s including play); the subsequent warm return is 9.16 s.

Final unrecorded High/Full static scenes average **204.4 FPS Intake, 224.2 Tower,
202.4 Discharge**, using 1,200 warmup and 1,200 measured frames per scene. Logs
confirm actual Immediate presentation after startup Fifo, 1280×720 physical
drawable, V-sync and all frame caps off. Each mean exceeds the current 120 Hz
display; 80–83% of loop intervals are shorter than one refresh. Earlier samples
vary substantially, so this is no game throughput improvement claim.


## Verification and delivery

Final receipts, source custody, per-gap assertions, Library identities, exact-head
CI, binary hashes and post-clean smoke are recorded in
`local/editor-annex-0.12.0/handoff.json`. All work stays on `0.12.0-staging`.
No accepted campaign source, asset, menu/distribution rollout, audio stage,
environment hub, main merge, release or branch deletion is included.

Native authoring verification uses visible controls in an isolated copy, with
focused-window document checkpoints and framebuffer captures. It is scripted
input, not a claim of manual mouse work. Complete source recovery preserves all
ten declared files exactly (manifest, three levels, three Luau scripts and three
thumbnails). The exercise retains a UI-authored upper-floor legacy route and a
water-opacity edit; accepted campaign sources and assets remain unchanged.

The final 960×544 logical-window exercise covers all twelve grouped controls,
invalid numeric input, keyboard focus, global/local ownership, preview/cancel,
apply/undo and the correction/ignore cycle. Ignoring the exact duplicate wall
removes its fix control; clearing that ignore restores correction eligibility.
Repeated corrections and undo return the original source. Build cancellation
retains the previous published package byte-for-byte. The first duplicate probe
used different scalar representations and correctly offered no exact-duplicate
fix; failed automation recipes are retained.

The three added owning-component CPU regressions protect local-floor ownership
and source roundtrips, upper-storey route continuity without falling to lower
support, and complete guidance wrapping with established Return dismissal.
Existing package and stacked-physics tests extend title and contact assertions.
They use small deterministic fixtures, take no GPU/assets or map bake, and the
full 170-test Rust suite takes about 0.35 s. Strict debug and release Clippy,
locked all-feature release build, formatting and patch whitespace pass. All 369
Python tooling tests pass with three supported skips; 610 assets validate with
zero warnings. Explicit adversarial package checks pass 49 cases and Luau release
checks retain eleven bounded refusals.

Remaining boundaries are deliberate: local-floor conversion refuses ambiguous
same-plane carriers, intersecting slopes and gabled ceilings; advanced Luau
logic and separate navigation AI remain available. Contact diagnostics concern
the authored rigid pose, not every animation/door/held trajectory. Connected
shaft candidates require contextual review, and the checker makes no exhaustive
closure proof. Validation here is native macOS/Metal; broader cross-platform
editor issue #18, menu issue #6 and general detail-panel issue #5 remain open.
No matching narrow issue was fully completed beyond those broader scopes.
