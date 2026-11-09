# Beginner’s Guide to Your First Places Space

Open [the illustrated PDF](../Beginners_Guide_to_Your_First_Places_Space.pdf).
The 28-page walkthrough targets the native Places 0.13.1 editor. It starts with
New, builds three rooms in one level, and ends with an exported, playable space.
PDF bookmarks provide navigation. Read at 100% or zoom into the original-resolution
screenshots; the larger control crops make menu labels easier to locate.

## Editable files

- `source/pages.json`: all prose, numeric tables, image selections and red callouts.
- `source/build_pdf.py`: ReportLab layout; crops and red arrows/circles are PDF
  overlays. The original PNGs are unchanged.
- `screenshots/`: native editor/player captures. Matching JSON files retain visible
  control coordinates and a small selection of checkpoint metadata.
- `example-project/`: the finished editable project created through the editor.
  Copy this folder to a writable location, then open its `places.project.json`
  with Places Editor. It is a reference result, not a prerequisite for following
  the guide. Generated `.placesmap` packages are intentionally not committed.
- `verification/`: compact native checkpoint receipts and screenshot hashes.

To regenerate from the repository root:

```sh
python3 -m venv /tmp/places-guide-venv
/tmp/places-guide-venv/bin/python -m pip install reportlab pillow pymupdf
/tmp/places-guide-venv/bin/python docs/editor-guide/source/build_pdf.py
/tmp/places-guide-venv/bin/python docs/editor-guide/source/validate_pdf.py /tmp/places-guide-review
```

Authoring used ReportLab 5.1.0 and Pillow 13.0.0. PDF validation uses PyMuPDF.
The validator renders every page, checks page bounds and required text, and
writes a report. Inspect its rendered pages after any content/layout change.

## Native verification and provenance

Verified on macOS arm64/Metal with Places 0.13.1 binaries built from
`8692c2524527539bc986c07c71f1f4a5b8306361`, on 2026-11-09 UTC.
The release task granted exclusive native-window slots; state, config and caches
were isolated under ignored `local/editor-guide/`. No shared target was cleaned,
no production code changed, and no release map was edited.

Editor automation used the existing `--verify-script` native control, text,
pointer and camera actions, invoking the actual editor handlers. Numeric walls,
rooms, columns, materials, props, two fitted doors, an open doorway, the recessed
basin, water, five ceiling fixtures and three linked elevators were created and
saved through that UI. The source was reopened across authoring stages. The new
project was initialized with the editor's `--new` path; the platform directory
chooser itself was not automated. Screenshot order is selected for instruction;
some previews show objects that the reader adds later.

The actual **Test in editor** session started and returned successfully. Native
**Final build** reported success, then **Project > Export all levels + sources**
produced a 3,842,067-byte package. The final source includes all three pillars.
The player loaded that exported package in isolated state. The Explore import
picker is supported by the implementation but was not separately exercised;
package loading and gameplay were verified directly.

The native player route used ordinary movement, view/aim and interaction inputs:
Backrooms → Home → Pool → Backrooms. The receipt records arrival positions.
The independent Home-door, closet-door and shallow-basin checks used diagnostic
starting poses followed by normal interaction and movement; no teleport was used
to cross a doorway or basin edge. The Home door passed at Z 1.717. The closet
passed at X 26.290, Z 0.600 after stepping back and re-centering following the
swing. Basin eye height changed from 1.6 to 1.2 and returned to 1.6. The duck was
visually checked resting on the bench. These checks do not claim every ordered
pair of elevator destinations was independently ridden.

## Scope and choices

The three destinations are rooms in one level, linked by the supported
three-cabin elevator group. The kitchen and duck are static props. The pool is
shallow enough to walk out; this tutorial does not require a ladder or swimming
system. Rooms provide floor and ceiling, so walls are authored explicitly.
Doors use the editor's built-in interaction setup; no entity or Luau authoring
is required. Windows key equivalents are included, but these screenshots and
native runs were made on Mac, not Windows.

All pages were rendered and visually inspected, including close-up menu labels,
red callouts, tables and finished player views. See `verification/pdf-validation.json`.
Rust checks were not rerun for this documentation-only deliverable; the release
owner retains responsibility for the release's code/build gates.
