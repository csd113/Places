# Beginner’s Guide to Your First Places Space

Open **[index.html](index.html)** in a current browser. If you downloaded the ZIP,
extract the whole folder first, then double-click `index.html`. Keep the `images`
folder beside it. The book works without an internet connection or a web server.

Start at lesson 1 and use **Next** at the bottom of each lesson. The contents list
lets you return to a section later. On a narrow screen, open **Contents** at the
top. Click a screenshot to enlarge it; press Escape or Close to return. The
full-resolution link below each picture opens the original-sized image.

The 34 lessons build one level containing Backrooms, Home and Pool, with three
linked elevator cabins. The editor stays in its **Simple** workspace throughout.
Walls, openings, doors, room resizing, props and lights use the visual tools.
Room creation, the initial pillar, the recessed basin and water use the forms
that Simple actually provides. There is no entity or Luau authoring.

The downloadable ZIP contains the same book and example project as this folder.
Its screenshots retain their native pixel dimensions with slightly lighter WebP
compression. The repository images use quality 92; the ZIP uses quality 88.
Red arrows and circles are separate SVG overlays, so their edges remain sharp
at any zoom. No editor controls or scene contents were painted into the images.

## Editable source and example

- `source/guide.json` contains the complete text, screenshot choices, captions and
  control callouts.
- `source/screenshots.json` retains image dimensions, native control positions,
  Simple-mode evidence and original capture hashes.
- `source/build.py` regenerates the book using Python’s standard library.
- `images/` contains 99 actual editor/player screenshots, used in 100 placements.
- `example-project/` is the result of the second fresh-project walkthrough.
  Copy this entire folder somewhere writable. In the editor, choose **Open** and
  select that copied folder. It is a reference result, not a prerequisite for
  following the guide. Use Final build and Export to make your own playable copy.
- `validation.json` records construction, build, import, gameplay and document
  checks, with hashes for the retained local replay evidence.

To regenerate after editing the source:

```sh
python3 docs/editor-guide/first-space/source/build.py
```

## What was verified

The written sequence was replayed from a second new project on macOS arm64/Metal
with Places 0.13.1. It did not import the first project's geometry or write scene
JSON. The native verification harness invoked real editor controls, typed form
values, and performed pointer clicks and handle drags. World-anchored pointer
inputs made those visual actions repeatable; camera actions only framed the view.
The replay reopened the saved project between its 27 construction stages.
All construction checkpoints reported `simple_mode: true`.

The second replay initialized the blank starter with the editor's `--new` option.
The native New/Open folder chooser was exercised separately during the initial
workflow investigation. Thus the replay is a fresh-project check of the written
sequence, not a claim that a second person independently followed it unaided.

Test in editor started and returned to editing. Final build and Export all
levels + sources succeeded. The exported package was imported through the game's
actual Explore → Import levels action in fresh isolated state. Ordinary movement
and interaction then verified Backrooms → Home → Pool → Backrooms, passage through
Home's door and the closet door, a clear office route, and walking down into and
back out of the basin. No player-position override, noclip or teleport was used.

Every lesson was rendered and visually reviewed. Navigation, screenshot zoom,
image loading, desktop bounds and a 390-pixel-wide layout were checked in Chromium.
The downloadable archive was extracted and checked separately. See
`validation.json` for the concrete results.

## Scope

The pool is shallow; no swimming or ladder is needed. The duck sits on the deck
beside the bench. The kitchen props are decorative. All three rooms occupy one
level and use one linked elevator group. The verified ride is the three-room
loop; this does not claim every ordered pair of destinations was separately ridden.

The Mac bundle and portable executable names were checked against the release
payload. The Windows executable name was checked against release packaging;
Windows was not run for this guide. No separate Mac editor app is packaged.

The earlier PDF and its source remain historical material in the parent folder.
This book replaces that incomplete walkthrough; the old PDF is not the current
guide. No production code or shipped map was changed for this documentation work.
