# Places sound library

The runtime reads `catalog.json` and the committed PCM WAVs in this directory.
There is no asset generation or download during startup, level loading, or normal
compilation. The 30 cues have 68 selected takes totaling 11,481,572 WAV bytes.

Imported source recordings are **CC0-1.0**, verified against the authors' linked
primary pages on October 16, 2026. Original Places ambience and elevator cues use
the repository MIT license and retain their offline recipes. Credit Kenney, rubberduck, OwlishMedia, TinyWorlds,
Peludo, Longor, isaiah658 and csaszi. Peludo's requested project reference is
<https://rnan.itch.io/>. Retain this directory's provenance and license records
when distributing the library with Places.

- `catalog.json`: cue IDs, variant files, restrained linear gains, looping and
  spatial defaults. File names are relative to this directory.
- `provenance.json`: authors, source/license/download URLs, pinned archive or
  download hashes, exact archive member names and source-member/output SHA-256.
- `authoring.json`: exact source selections, trims, crossfades and peak targets.
- `licenses/`: original Kenney license notices and the complete source credits.

See [the audio asset guide](../../docs/assets/AUDIO.md) for cue purposes, authored
examples, budgets, offline integrity checks and the explicit import workflow.

The elevator additions are original Places audio under MIT, rendered offline by
`tools/audio/elevator.py`: a short traction-motor/cable shaft pass and a restrained
arrival ding. They are committed mono 44.1 kHz PCM16 WAVs, preloaded with the
ordinary library; the exact recipes and output hashes are in authoring/provenance.
The current complete audit reports 30 cues, 68 WAVs and 43.79 MiB decoded stereo
float samples, within the 64 MiB budget. No runtime synthesis is involved.
