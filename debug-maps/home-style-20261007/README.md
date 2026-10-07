# Home reconstruction review payload

The tracked visual journal is [the Home entry](../../docs/style-upgrade-20261007/home/README.md).
Its before/after PNGs are unaltered native Metal captures with matched cameras.

Local `evidence/` preserves independent playable snapshots outside `target`:

- `before/`: starting Pool commit assets, demo package, renderer and twelve captures.
- `candidate/`: first complete Home iteration, native captures and frozen assets.
- `candidate2/`: second twelve-view review, before the casing overlap correction.
- `candidate3/`: darker two-fixture fill experiment rejected after native review.
- `candidate4/`: short-range globe review used to calibrate the tall-vault fill.
- `candidate5/`: range-correct review before final collision-stock and wall-mount fixes.
- `lamp-check-off/`: private package changing only the kitchen pendant enabled flag.
- `after/`: final assets, packages, renderer and twelve matched captures.
- `model-previews/`: construction review images; these are not gameplay evidence.
- `validation/`: commands, raw compiler logs, CSVs, audits and sleep assertions.

To revisit a snapshot, use its frozen binary and assets with
`tools/bench/capture_home.py --root <snapshot> --binary <snapshot>/bin/places
--out <new-output-directory>`. Existing evidence is never overwritten.
The private payload is locally excluded from Git; its matched native PNGs,
camera manifests, summaries and provenance are committed in the journal.
Older Pool/debug archives and reusable build artifacts are preserved.
