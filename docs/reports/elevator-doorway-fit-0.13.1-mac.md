# Elevator doorway fit, cabin seams and automatic editor placement

The metal doorway now meets both wall planes. The landing frame finishes at
local Z=0 inside the 1.50 × 2.25 m host opening; the passenger frame finishes at
Z=0.360 m beside the retained walnut front wall. Both sliding steel skins are
20 mm thick, with a concealed 4 mm centre seal. The original 0.150/0.300 m track
anchors, 0.681 m eased travel, conservative collision envelopes, controls,
lighting and travel behavior are unchanged by the artwork commit. Each fully
open panel leaves a 7 mm lip beside its jamb. Closed panels overlap concealed
rebates at the centre, both sides, header and backed sill.

The former projecting frame and detached inner stops are replaced by fitted
stock. Frame triangles fall from 386 to 240, and cabin triangles from 1,336 to
1,324; leaves remain 56 triangles each. Three button modules produce a
1,486-triangle cabin. This is geometry/budget evidence, not an FPS claim.
No textures, buttons, lights, handrails or cabin finishes were redesigned.

The separately requested cabin-corner inspection found a real 25 mm gap under
side/rear bronze skirting and small unjoined rear corners. The three existing
skirts now reach the floor and meet continuously. Other panel shadow reveals
retain their opaque backing. Editable Blender sources and exports are retained.

## Editor behavior

Build > Elevator resolves one pose for preview and click from the actual wall
face and thickness, including horizontal rays through existing apertures. The
wall-normal coordinate never goes through world-grid rounding. All four wall
facings are automatic; the along-wall coordinate follows local snapping or the
centre of the existing doorway. Adding the lift and fitting/creating its opening
is one source edit, with one Undo/Redo. Existing panes, recesses, surrounds and
hinged leaves receive a specific refusal rather than being overwritten.

Placement checks the real two-section envelope: 2.90 m wide shallow pockets and
a 2.56 m wide rear body extending 2.48 m behind the landing face. It rejects
intersections with adjacent rooms, solid wall slices, other elevators and source
room ceilings. Boundary contacts within 0.5 mm are allowed. A sufficient gap is
not rejected using the former coarse three-metre square. Existing shared
candidate/shaft validation also runs before the preview is considered valid.
Host walls deeper than 0.24 m are explicitly refused: the retained rear cabin
body starts at Z=0.24, and shared shaft admission starts at Z=0.25. The editor
does not alter a room floor/ceiling or weaken runtime collision to force a fit.

## Evidence

Artwork commit: `6e9d328f`, based on `9a771f1b`. All receipts, actions and raw
captures are under `local/doorway-polish/` in the task checkout.

- `check_seals.py`: 21,546 closed sightlines across combined, moving-car-only
  and landing-only assemblies; zero leaks. All 155 open-aperture rays pass,
  with visible lips independently checked from both sides.
- `check_surface_joins.py`: no exposed coplanar overlap at five door poses.
- `check_uv_stability.py`: no UV violations or microscopic weld slivers.
- `check_frame_materials.py`: all tested returns/frame surfaces remain metal;
  all nine surrounding-wall probes remain walnut.
- `check_doorway_fit.py --baseline 9a771f1b`: 18 actual-surface probes meet the
  exact two wall planes. Protected triangle/UV comparisons exclude only the
  doorway and explicitly authorized bronze floor skirts; the button and atlas
  remain byte-identical.
- `check_cabin_corners.py --baseline 9a771f1b`: 302 shell probes and 27 close
  floor-join probes, with 48 baseline leaks reduced to zero.
- `final-corners-v2/corners/`: 16 direct native Metal captures using the final
  artwork package, including every floor and ceiling corner, normal floor view,
  and inside/outside open/closed/oblique doorway views. These were visually
  inspected. The package SHA-256 is
  `d3e601d94f52164f4e1ef22eeb6918690e62c7ba52ffb1f28bc9513c2c079188`.
- `final-outside-cycle/` and `final-inside-cycle/`: 409 and 413 lossless
  native frames of opening, real manual closure and reopening, using the same
  final-art package. Both assert fully closed at the 3,700 ms checkpoint and
  fully open at 6,100 ms on the same floor. Mid-motion and closed/open endpoints
  were inspected from both sides. The exact local driver and actions are retained.
- `final-outside-motion/` and `final-inside-motion/`: additional 151/350-frame
  final-art opening/physical-arrival sweeps. Earlier `outside-left/` and
  `inside-right/` runs predate the final skirt/bevel refinement and are not the
  final visual acceptance.
- `native-editor-v3/`: actual pointer preview and horizontal wall clicks place
  four cabins at 0/90/180/270 degrees into .10/.20/.24/.20 m walls. Face origins
  remain exact off-grid; along-wall placement uses the existing .25 m grid.
  A 2.47 m gap and .35 m host wall are refused with the source unchanged.
- `fitted-game/`: the exact saved native-editor project is explicitly exported
  and played directly. All four real call buttons open the cabins; 16 inspected
  inside/outside closed/open captures and a contact sheet show the fitted stock.
- `flow/`: physical travel, save/resume, frozen loading, both cross-level
  directions and return passed on the doorway build before the skirt repair.

The four low-eye floor close-ups in the first `final-corners/` attempt are
explicitly excluded: the full-height player body fell below the floor after
being positioned at 0.45 m eye height. The corrected v2 uses valid standing
positions and shows all four floor corners clearly. The first editor wall-fit
CPU attempt also correctly exposed the existing 0.25 m shaft clearance limit;
the final wall-thickness gate preserves it.

Lighting-worker enclosure/receiver updates are a separate coordinated change.
Geometric sightline closure here does not claim that an unshadowed dynamic
light cannot pass through stock. Integrated lighting acceptance belongs with
that worker's current shader and final runtime build. These native results are
bounded macOS Metal coverage; no cross-platform or uncapped throughput claim is
made.

The initial editor harness assumed a .50 m along-wall grid; its receipt was
corrected to the editor's actual .25 m step. Its fourth-facing run exposed a
real selection bug: an eye-level ray could select a distant cabin through a
wall when no floor ray existed. Selection now stops at the nearest wall, and
the complete corrected v3 campaign passes. The test suite remains source-only;
none of these explicit map exports or native runs enter ordinary CI.

Strict workspace/all-target/all-feature Clippy and all **444 workspace tests**
pass on the final editor source. Three small placement regressions protect
four-face/off-grid fit plus atomic Undo, actual depth/width/height boundaries,
and refusal of an existing hinged door without source mutation. The existing
linked-group regression still passes. Asset, prop, texture and formatting
checks pass, with the existing 17 source/asset and 118 texture advisories.
