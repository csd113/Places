# Places documentation

Start with [architecture](ARCHITECTURE.md), [renderer](RENDERER.md) and
[desktop verification](VERIFICATION.md). Map and asset work follows the
[map authoring guide](MAP_AUTHORING_GUIDE.md) and
[asset specification](ASSET_SPECIFICATION.md); the
[package format](PACKAGE_FORMAT.md) defines archive and safety contracts.

The [seven-stage Art-style record](style-upgrade-20261007/art-style-history.md)
and [completed integration](art-style/stage7/README.md) preserve the historical
results and their limits. The separate
[model-lighting correction](model-lighting-root-cause-and-fix.md) documents the
later kitchen, corridor and skeleton defects, their causes and matched native
before/after PNGs. Selected historical captures are under `images/`; individual
final reports in `reports/` explain their own accepted findings and limitations.

## Documentation cleanup

On October 9, 2026, final reports were consolidated before retiring generated
logs, trace dumps, receipts, intermediate narratives and redundant captures.
The original inventory contained 6,517 files and 2,050,460 text lines, including
the corrective mission's pending evidence. The retained guides, final reports,
107 curated genuine PNGs and one functional asset preset occupy approximately
21,800 text lines. No source assets or requested playable custom debug maps
were removed.

Reusable camera/sequence/ray inputs now live in `tests/fixtures/native/`.
The former renderer's 50 original PNG comparison inputs are preserved byte for
byte in `tests/fixtures/native/gles2-reference/`. Tools and tests use those
locations; generated capture output goes to ignored debug/verification folders.
These are functional test inputs rather than published audit dumps.

The 4,666 retired historical files (727,416,245 bytes) were moved to genuine
macOS Trash and hash-verified there. Their single recoverable payload is named
`retired-historical-docs-20261009T1600Z`, with repository-relative originals,
`RESTORE.md` and an exact per-file SHA256 restore manifest. Unique controller
sources, prompts and motion videos remain recoverable in that payload.
No unique evidence was permanently purged. The 3,280 superseded corrective files (4,954,083,519 bytes) were likewise
hash-verified in `retired-corrective-evidence-20261009T1650Z`. Both payloads retain
their restore manifests. The active final runnable bundle, replay controls,
curated before/after PNGs and compact accepted verification summaries remain
outside `target`; redundant campaigns are no longer active workspace clutter.
