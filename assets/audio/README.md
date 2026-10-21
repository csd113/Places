# Places sound library

The runtime reads `catalog.json` and the committed PCM WAVs in this directory.
There is no asset generation or download during startup, level loading, or normal
compilation. The 25 cues have 63 selected takes totaling 5,519,032 WAV bytes.

All source sounds are **CC0-1.0**, verified against the authors' linked primary
pages on October 16, 2026. Credit Kenney, rubberduck, OwlishMedia, TinyWorlds,
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
