# Office collection migration

Office remains the same three-storey building: entry offices at Y7.2, displaced
middle corridors at Y3.6 and the basement at Y0. No additional upper storey is
introduced. The approved R2 mannequin and four cockroaches retain their original
basement AI, bounds, refuges, clips and lighting response.

## Route correction

The old east shaft put both flights on one south landing. Players could descend
the upper flight and immediately turn onto the basement flight without entering
the middle rooms. The basement flight now occupies a separate enclosed west shaft
(X−8…−4). Its 3 m wide ramp runs Z4…14, descending from Y3.6 to Y0. Short north
and south connector corridors meet the middle and basement, respectively.

The vacated east shaft floor is raised to Y3.6 and its former basement opening is
closed. The upper flight retains its world position. A player who jumps over its
rail can therefore reach only the middle-level shaft floor, rather than bypassing
the middle and falling directly to the basement. The west ramp avoids the upper
vestibule footprint, respecting the engine's single-carrier-plane ramp contract.

Every shaft railing is regenerated through the existing file-backed module
linker using actual carrier/slope support. Its 44 posts have 396 sampled footprint
contacts, with bases embedded 6–53.52 mm into support. The two routes retain
2.86 m between rail centrelines and about 2.74 m between their inside faces,
more than the player's 0.6 m diameter. Fixed
fixtures illuminate the moved slope and both landings; the existing eastern
lower wall light moves above the raised floor. Standing clearance is at least
2.8 m at the west ramp's high landing and increases down the flight.

## Interaction and physics

The paired drinks/snack power interaction retains its steady, flickering and dark
states, shared switch group, authored emission overrides and independent basement
lamp failures. The two CRTs, mug and books now explicitly use their reusable
`assets/gameplay.json` physical profiles and the `office_desk_items` reset group.
They start dormant at their authored desk poses. A compact vestibule wall switch
restores those four items through `reset_physics_group`; no furniture is turned
into a loose object. The mug's old fixed-placement E/tap binding is retired in
favor of actual pickup; its allowed audio emitter remains. Movable props cannot
own a fixed-placement interactable or event bindings, and the first production
validation explicitly caught that conflict before the source was accepted.

`office_return_hub` is a real closed door in the upper vestibule's west wall. Its
hinge is world `[−4,7.2,9.7]`, closed rotation 90°, width 1.4 m, and centre
`[−4,8.375,9]`. The authored entry spawn remains `(−2,9)`, facing east (90°), away
from the return door. The collection controller owns its transition binding.
The 4:1 return plate is 1.2 × 0.3 m, centred at world Y9.81. Its Y9.66–9.96
extent clears the actual casing top at Y9.62 and the Y10 ceiling by 4 cm on each
side. This avoids hiding the plate's lower border behind the proud door frame.

## Reproduction and evidence

The migration starts from the actual editor-saved import, supplied explicitly to
`tools/levels/migrate_office_collection.py --source IMPORTED --out
projects/environments/levels/office.json --report RECEIPT`. The helper refuses the
standalone source path and never overwrites it. This deliberate geometry redesign
is separate from automatic import correction; it preserves unrelated source
fields, per-placement review metadata, intentional sunken furniture and existing
AI. No new artwork or blanket geometry ignores are introduced.
The helper is the initial migration recipe, not a generator for the final project;
reproduce into an isolated output rather than overwriting later hub wiring.

The original standalone source's production geometry scan reports zero errors and
one bounded `room-leak` candidate at `[24.875,0,14.125]`, beside the old shared
landing; two existing narrow `missing-wall` suppressions remain. The actual
editor import preserved the original source bytes and passed save/reopen and
asset validation, with 12 review entries and no eligible automatic corrections.
The final migrated source passes the strict production geometry scan with **zero
errors and zero warnings**. One additional narrow `missing-wall` annotation
identifies the west landing's intentional opening onto the physically railed
ramp; it does not suppress leaks, geometry defects or model findings. The first
trim revision produced six slivers, corrected by joining the former doorway's
boards and clipping at the actual perpendicular basement partitions.

Local receipts are `local/environment-collection-0.12.0/office-migration.json`,
`office-migrated-geometry.json`, and the import's
`import-audit/imports/office_demo/receipt.json`. The collection's combined handoff
records the later build/package validation, focused native routes and photographs.

A read-only final actual Document/Safety review in `import-audit/final-office/`
reduces the 12 baseline entries to 11. Both CRT collision reviews and the old
shaft leak candidate are resolved. Eight contextual model-plane candidates
remain: four tiny west railing corner contacts (0.00013 m² each), two existing
filing-plinth contacts, and CRT/desk plus mug/desk support contacts. The module
linker deliberately embeds rails into support sides. These candidates remain
visible for native review; no placement ignore or automatic move is applied.
The model triangle pair budget remains incomplete, alongside the scope note and
12 measured floor-contact notes. This bounded review is not a complete model
intersection or visibility proof.

Normal-input route waypoints, with feet heights:

| Section | Waypoints (X,Z) | Feet Y |
| --- | --- | --- |
| Top entry to upper flight | (−2,9), (20.3,9), (20.3,6), (22.5,2), (32.5,2) | 7.2 |
| Upper descent | (32.5,4), (32.5,14), (32.5,15.8), (22.5,16) | 7.2→3.6 |
| Middle through-route | (21,11), (21,7), (7.8,7), (7.8,9.8), (3.7,9.8), (3.7,1.5), (−6,1.5) | 3.6 |
| Basement descent and entry | (−6,4), (−6,14), (−6,15.9), (2,16) | 3.6→0 |

## Early native Static evidence

The isolated one-map projects in `local/environment-collection-0.12.0/office-static-fixture/`
use the frozen production Office source, with only its unavailable hub-return
binding and transition declaration omitted. The separate AI-ramp project also
widens only the mannequin's `ai.home_bounds`; its geometry and authored actor pose
remain unchanged. Both passed normal project checks, package validation and
current-source verification. Compiler `9bb1abca…` built Static only with two
workers; native player `8070dc5e…` ran the following focused-window checks.

| Native receipt under `local/environment-collection-0.12.0/native/` | Result | What it establishes |
| --- | --- | --- |
| `office-static-natural-final-01/acceptance.json` | 91 checks passed | Ordinary held movement and camera input reached the upper east landing, crossed the middle rooms, descended the moved west flight and entered the basement. All five shipped agents moved and retained Y0. No player recovery occurred. |
| `office-static-ramp-edges-final-01/acceptance.json` | 57 checks passed | Explicit initial diagnostic player poses followed by normal movement/jump input retained the correct side and height at the tested ramp edges, with zero recovery. |
| `office-static-bounds-guard-final-01/acceptance.json` | 11 checks passed | The shipped mannequin's original basement home bounds refused the west-shaft goal. This is a bounds check, not evidence of a ramp climb. |
| `office-static-ai-ramp-final-01/acceptance.json` | 50 checks passed | In the bounds-only variant, production navigation physically moved the mannequin from the basement to the ramp, up to Y3.6 and through the upper portal into the middle connector. No actor/player pose action moved it there. |

The ordinary route reached approximately `(32.468,3.6,15.771)` at the east
landing, `(3.672,3.6,1.409)` at the middle west approach,
`(−6.038,0,15.830)` below the moved ramp and `(1.971,0,15.930)` inside the
basement. Coordinates here use feet Y. The bounds-only actor progressed through
Y0.922 and Y3.331, then reached `(−5.956,3.6,2.579)` on the upper landing and
`(−1.952,3.6,2.105)` in the connector, with no recorded stuck time. The shipped
mannequin remains intentionally basement-only; it does not gain this diagnostic
cross-storey behavior.

The side/jump probes stopped at X34.300 on the upper flight's eastern approach
with feet Y3.6. The west ramp's two probes stopped at X−4.928 and X−7.072 with
feet Y1.8. These bounded contacts establish the tested cases, not every possible
jump. Ambient, flashlight and opposite-side Static close-ups show the sampled
posts meeting their support without an obvious hover gap. Still images do not
establish absence of temporal shimmer; all model-contact candidates and the
incomplete pair-budget warning remain reviewable.

Final **a9 Full collection** validation, ordinary route replay, hub→Office→hub,
lighting/composition review and uncapped performance remain pending. The Static
fixtures intentionally cannot verify hub return and provide no Full visual or
performance acceptance. The recorded runs also do not exercise the new desk-item
reset switch or the preserved vending cycle; those interactions must not be
reported as newly native-verified. The source route is reversible, but these
Static receipts establish the downward ordinary traversal only. Final acceptance
is owned by the combined collection pass before standalone package retirement.
