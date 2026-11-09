# Editing and verification

`guide.json` contains the shared text, screenshot order, captions, checkpoints
and placement of each image in the instructions. `build.py` generates the seven
GitHub Markdown chapters and the single PDF. The `images/` directory contains
actual native editor/player screenshots with red control callouts embedded;
`-detail.jpg` files are reproducible crops of those same annotated screenshots.
The full frames retain their native aspect and dimensions. PDF overview images
use a 1600-pixel maximum edge; control details use native-resolution crops.

To regenerate, install `reportlab`, `beautifulsoup4`, `markdownify` and `pillow`
in a Python environment, then run:

```sh
python3 docs/editor-guide/source/build.py
```

The default fonts are macOS Arial files. On another system, use `--font-dir`
with a directory containing Arial.ttf, Arial Bold.ttf and Arial Italic.ttf.
Use `--pdf` to select another output path. After an edit, render and inspect the
PDF, check all relative links, and verify the public download before publishing.

## Verified walkthrough

The content reuses the Places 0.13.1 fresh-project replay recorded on Mac:
27 saved construction stages, all in Simple mode, with 3 rooms, 16 walls,
3 pillars, 9 props, 7 ceiling lights, 2 fitted doors, a recessed basin, water
and 3 linked elevators. The example project is the saved result of that replay.

The automated replay used real editor controls, typed fields and pointer/handle
actions. Camera positioning only framed the view. It started with a blank
`--new` project, did not import the first project's geometry and did not write
scene JSON. New/Open folder selection was checked separately. This is a second
fresh-project replay of the written sequence, not a second-person usability test.

Test in editor, Final build, complete export and fresh game Import levels
passed. Normal movement verified Backrooms → Home → Pool → Backrooms, both
doors, the office path and basin entry/exit. No player-position override,
noclip or teleport was used. The pool is shallow and needs no ladder. Not every
ordered pair of elevator destinations was separately ridden.

Mac app and portable paths were checked against release packaging. The Windows
executable name was checked against the packaging script; Windows was not run.
This delivery changes documentation organization and format, not the tested
scene or editor workflow. No new native run is claimed for this format revision.

The PDF has 34 numbered tutorial sections, chapter/section bookmarks and linked
contents. All 100 screenshot placements are embedded. The build does not fetch
images, fonts or content from the internet. The guide can be read on GitHub or
as the one downloaded PDF; no repository checkout is needed to follow it.
