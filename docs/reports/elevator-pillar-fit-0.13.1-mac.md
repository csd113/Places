# Elevator host volume and pillar placement

This revision supersedes the 24 cm host-wall limit. Depth alone no longer
rejects a wall: the editor hollows the selected host for the doorway, paired
sliding pockets and cabin. Adjacent rooms and other architectural solids still
bound the available volume. The actual envelope is 2.90 m across the shallow
pockets, 2.56 m across the rear body, 2.48 m behind the flush landing face, and
2.63 m tall. Existing elevator overlap and shared shaft admission remain active.

Rectangular `columns[]` accept entry on all four cardinal faces. Circular
`pillars[]` derive a fitted flat entrance from the authored polygon and retain
an arc shell, body material, outer radius and a cap. Their usable interior must
contain the cab inside the retained 10 cm curved shell, with a small collision
clearance. Small hosts receive a clear **Not enough space** refusal. The editor
saves ordinary wall/column/arc records; runtime rendering, collision and export
consume the same hollow geometry. One Undo restores the original solid host.
No elevator artwork, movement, controls or lighting code changed.

The editor only alters the selected host. Existing bare wall apertures are
preserved, while attached panes/recesses/surrounds and hinged doors receive their
existing explicit refusal. In-room decks get a 12 mm underlying floor recess
when no different storey overlaps; stacked floors are left untouched. Selecting
hidden host geometry is filtered by the active editor focus.

## Validation

- Strict workspace/all-target/all-feature Clippy passes (`pillar-clippy-final.log`).
- All **446 workspace tests** pass (`pillar-tests-final.log`). The added tiny
  source-only regression covers 60 cm wall depth, large/small rectangular and
  circular pillars, real shaft admission, save/Undo/Redo, and a stacked floor
  that must remain unmodified. Existing adjacent-room depth/width/height,
  wall-facing, terrain-picking and elevator-link regressions continue to pass.
- Preliminary direct native editor campaign (`pillar-native-v3/`) created eight
  valid lifts: four wall facings (10, 20, 35 and 60 cm), two rectangular pillars,
  and two circular pillars. Two undersized pillars and a 2.47 m room gap were
  refused; 2.48 m room gaps fit. Its rotated-pillar assertion initially expected
  the wrong sign of along-wall snapping; the product pose was correct and the
  harness expectation has been corrected for the final rerun.
- The editor-saved project exports successfully with no warnings
  (`pillar-export.json`, `pillar-game.placesmap`). It was opened in the direct
  game binary. Thirty-two open/closed inside/outside captures plus eight wider
  shell/floor-corner views were retained and representative views visually
  inspected (`pillar-game-run/`, `pillar-side-run/`). Cabins are not filled with
  their original solid host; frames meet the fitted wall face and floor joins
  are continuous. A contact sheet is `pillar-contact-sheet.png`.
- The fixture uses Static preparation and macOS Metal. These are geometry and
  behavior checks, with V-sync enabled; no throughput/FPS improvement is claimed.

## Final production acceptance

The locked all-feature workspace release build passed in 6m 13s
(`pillar-release-final.log`). The final production editor passed the complete
11-case native campaign (`pillar-native-final/passed.json`): eight placements,
two small-pillar refusals and one short-gap refusal, with exact automatic wall
facing/face alignment. Its saved source equals the preliminary fixture source.
The saved project was exported again with zero warnings
(`pillar-export-final.json`, `pillar-final.placesmap`) and run in the final
production game. All eight lifts reach fully open in actual game checkpoints;
32 final inside/outside open/closed images are retained. Binary/package SHA-256
identities and checkpoint results are in `pillar-game-final/passed.json`.
`pillar-final-all-views.png` and `pillar-contact-sheet.png` summarize those views.
The additional wider-shell and floor-corner views remain in `pillar-side-run/`.

Native coverage is macOS Metal with a small Static fixture; it is not an FPS
comparison or a cross-platform campaign. This revision introduces no new map
schema or runtime boolean operation: the hollowed host is editable saved
architecture. Circular hosts gain a flat fitted doorway facade; their remaining
interior clearance is measured against the actual polygon and retained shell.
Existing specialized wall attachments remain explicit occupied-host refusals.
All local evidence is under `local/doorway-polish/` in task-6/places.
