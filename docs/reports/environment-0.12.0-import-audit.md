# Environment collection: legacy import audit

The eight distributed environment sources were imported through the new real
editor operation `Document::import_level`, also available as **Project > Import
JSON map**. The previous UI recovered source-bearing `.placesmap` projects only;
it had no standalone JSON import operation. Imports now validate bounded source,
unique keys, representable numbers, runtime semantics, project membership and
placement-intent metadata, then publish complete original bytes without replacing
an existing destination. Original stable IDs, unknown fields and formatting are
retained. Script dependencies require owning-project recovery; undeclared
destinations and duplicate IDs are refused without guessing replacements.

Evidence is under `local/environment-collection-0.12.0/import-audit/`.
`import-inventory.json` indexes original/migrated hashes, editable imported
projects, diagnostic files, save/reopen equality and installed-asset validation.
All eight migrated baseline hashes equal the original source hashes. No baseline
was automatically corrected, manually edited or given a new ignore. Existing
`geometry_intent` annotations remain exact: 288 records across the originals.
None of the eight originals contains an `editor_placement_intent` ignore.
The existing coordinate-based geometry annotations and the isolated placement
ignore exercise below are different evidence.

| Legacy source | Safety entries | Offline errors / warnings | Preserved intent records |
| --- | ---: | ---: | ---: |
| Pool Showcase | 56 | 3 / 1 | 0 |
| Home Showcase | 17 | 0 / 1 | 0 |
| Office: After Hours | 12 | 0 / 1 | 1 |
| Lantern Hollow | 1 (budget refusal) | 1 incomplete audit / 0 | 54 |
| Winter | 1 (budget refusal) | 0 / 2 | 45 |
| Beach | 1 (budget refusal) | 1 incomplete audit / 0 | 162 |
| Frutiger Aero | 1 (budget refusal) | 0 / 0 | 26 |
| Cosmic Lanes & Arcade | 50 | 0 / 34 | 0 |

The original Pool has three measured duplicate-surface overlaps at the coping
junction near `(8.875, 0.035, 7.625)`: about 0.0013, 0.0078 and 0.0484 m².
These involve threshold geometry, outside the conservative wall-removal autofix.
They were handed to the Pool redesign owner for an explicit geometry edit.
Home has ramp-side/drop and wall-cabinet collision review candidates. Office has
two CRT collision candidates and a lower-storey leak candidate. Arcade's offline
warnings are 34 repeated prop-placement candidates; they do not justify automatic
prop deletion because behavior and composition may differ.

Lantern and Beach exhaust the offline coplanar candidate budget. Those explicit
`geometry-check-incomplete` errors describe incomplete coverage, not confirmed
mesh defects. Interactive Safety also refuses maps over its object/work budgets.
Pool/Office model comparisons retain explicit pair-budget notes. No budget was
raised and no safe/closed-map conclusion follows from these bounded checks.

The originals contain no exact duplicate simple wall records, the only currently
conservative automatic correction. The original-map correction count is therefore
zero, rather than a claim that automatic correction repaired their findings.
A separate isolated **actual imported Pool** copy received two deliberately
duplicated simple walls. Actual Safety
preview left source unchanged; apply and undo restored exact source. One duplicate
was explicitly reviewed/ignored, saved and imported into another fresh project.
Exactly one active per-placement decision survived. Direct correction of the protected
wall refused, the batch candidate gate skipped it, and the independent duplicate
was removed while the protected pair remained. No correction candidate remained
afterward. `ignore-probe-receipt.json` records this complete exercise. The original
and migrated baselines were untouched.

All eight imported projects also pass the preserved direct `places-compile
check-project` operation with explicit repository assets. The first automated
`--events` attempt closed stdin and correctly cancelled with status 130; those
logs remain labelled separately. Plain CLI reruns all exit 0. No lightmap bake is
part of this import audit, Rust tests or ordinary CI.

Two small editor-owned regressions protect byte/intent preservation and rejection
of duplicate IDs, unresolved scripts/destinations, ambiguous JSON and existing
unowned files. They perform local tiny-source writes, without assets, GPU or a
map bake. At the import implementation checkpoint, both passed in 0.03 seconds;
the then-current workspace suite passed all 175 tests in 0.39 seconds of execution,
and strict all-target/all-feature Clippy passed. These are earlier checkpoint
results, not the final collection head's gate results. The parent stage owns the final release
build, native UI/gameplay validation, export/recovery roundtrip and exact-head CI.

An additional read-only review of the redesigned Office is retained under
`final-office-confirmed/`: 12 baseline entries become 11; the leak and both CRT-collision
warnings disappear. Model-coplanarity candidates increase from six to eight;
one model-pair budget
note, support-contact information and scope information remain. The offline
checker reports zero errors and warnings. No source correction or ignore was
applied by this follow-up. This Safety scan identifies source `68609855…`;
the later return-sign polish identifies final Office source `e1aa9307…` in
`office-migration.json`. Native composition review remains with the stage.

## Deliberate collection edits and measured changes

The byte-exact baselines are preserved separately from subsequent source
authoring. The Home/Pool and Office authoring helpers operate on those imports;
their geometry redesigns are manual authoring decisions, not editor autofix or
native placement-edit claims. `home-pool-authoring.json`, `office-migration.json`
and `destination-edits.json` record the changes:

* Home is a complete seven-room redesign using the approved domestic assets and
  R2 carry quest. Its chair's standable height is corrected to the actual seat
  surface so the requested cushion does not overlap a full-height collision box.
* Pool retains the original rooms and adds a second basin room and a separate
  janitor closet. The old threshold ends at X8.753, leaving a 2 mm joint before
  the perpendicular coping at X8.755; this deliberate trim resolves the three
  measured duplicate surfaces. No threshold-shape autofix is available. The
  reusable janitor cardboard profile also receives an explicit 0.024 m³ water
  displacement after native body admission exposed the invalid inferred value.
* Office moves the basement flight to the west shaft, raises the vacated east
  floor, connects both landings, regenerates supported rails and fits lighting.
  Four desk objects receive explicit dormant physics; the obsolete fixed mug
  interaction is removed after validation rejected that conflict. One narrow
  new `geometry_intent` annotation describes the physically railed west landing
  opening. The original bridge annotation remains. Final collection sources
  contain 289 geometry annotations and zero per-placement ignores.
* Every destination receives a physical return door, nameplate and explicit visit
  behavior. The other five migrations retain their layouts while fitting
  physical profiles and existing-asset themed interactions. These authored
  additions are not automatic import corrections.

| Check and evidence | Baseline → collection result | Remaining review |
| --- | --- | --- |
| Pool production geometry; `readonly-pool-audit-water-fixed/` | 3 errors / 1 warning → 0 / 0 | Safety still has 56 entries, including 31 exit candidates, 22 model-plane candidates and an incomplete model audit. |
| Office production geometry; `office-migrated-geometry.json` | 0 / 1 → 0 / 0 | Three narrow missing-wall findings are covered by its two explicit opening annotations. Earlier Safety comparison is 12 → 11, with model-plane candidates 6 → 8. |
| Home production geometry; `readonly-home-audit-fixed-seat/` | 0 / 1 → 0 / 2 after complete redesign | The two shared-boundary warnings remain. Safety entries increase 17 → 31, including 21 model-plane candidates, five nonsolid-prop reviews and an incomplete model audit. |
| Arcade production prop check; `source-geometry/arcade-checker-improvement.json` | 34 repeated-prop warnings → 0 | All 34 were distinct balloon component owners. The checker eligibility fix preserves the source placements; no deletion or ignore produces this result. |

Both latest Home and Pool scans report zero movable contact/unchecked findings,
preserve their source hashes and reach the unchanged 2,000,000-step model budget.
Their current source hashes match their read-only receipts. Safety totals include
scope, support and conservative review entries; they are not defect totals.

Across the nine collection maps, the saved production geometry scans, with the
later Arcade checker result replacing its earlier scan, contain two explicit
incomplete-audit errors (Lantern and Beach) and 17 review warnings: hub 13,
Home two and Winter two. No confirmed duplicate-surface error remains. Four large
legacy maps still exceed interactive Safety's budget. These are conservative
coverage limits, not automatic clearance or proof that every room is sealed.
`import-audit/final-supporting-evidence.json` records the exact evidence inputs,
source hashes, manual/automatic distinction and remaining limits.

## Native import and document roundtrip

`local/environment-collection-0.12.0/native-editor-import/receipt.json` records
15 passing checks and 15 checkpoints with actual SDL input focus. The direct
immutable release editor (`3e77420e…`) created a fresh project through `--new`,
clicked the visible **Project > Import JSON map** control, typed the absolute old
Home path, applied and saved. A separate native process reopened the imported
map and inspected its Project controls. Original/imported source hashes remain
`3f5caff7…`; semantics are exact. An isolated complete collection copy then
exercised transform preview, apply, undo, save and a separate native reopen.
All twelve project-file hashes returned exactly to their initial values and the
production project remained unchanged. No build/export/play was invoked.

The real Project screenshot exposed a left-column layout overlap between
**Selected map title** and **Selected / Next**. The smallest correction changes
the six left rows from 17 to 15 logical pixels apart. Buttons remain 13 pixels
high, text remains 7 pixels high, the gaps between rows are 2 pixels and the last
row ends at 155 before the next control starts at 159. The before screenshot is
retained. The fresh immutable release editor (`d87e9c66…`) verifies separate
title/next-map controls, opens the title form and switches maps with actual
focused input. `title-spacing-receipt.json` records all three focused checkpoints
and unchanged source hashes. A first input recipe redundantly requested the
already-open Project panel; its refusal is retained alongside the corrected,
successful recipe. All native processes are closed; no windows remain.
