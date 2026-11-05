# Places sound library

The runtime reads `catalog.json` and the committed PCM WAVs in this directory.
There is no asset generation or download during startup, level loading, or normal
compilation. The 32 cues have 70 selected takes totaling 14,242,320 WAV bytes.

Imported source recordings are **CC0-1.0**, verified against the authors' linked
primary pages on October 16, 2026, with the intercom addition checked November 5, 2026.
Original Places ambience and elevator cues use
the repository MIT license and retain their offline recipes. Credit Kenney, rubberduck, OwlishMedia, TinyWorlds,
Peludo, Longor, isaiah658, csaszi and GrimmReaperSounds. Peludo's requested project reference is
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
The current complete audit reports 32 cues, 70 WAVs and 54.32 MiB decoded stereo
float samples, within the 64 MiB budget. No runtime synthesis is involved.

`ambience_intercom` adds “The intercom of nothingness” by GrimmReaperSounds
(CC0, source page checked November 5, 2026). It uses the user-supplied MP3,
not a replacement download. The 10–40 s selection is downmixed to mono,
resampled to 44.1 kHz and crossfaded by 0.5 s into a 29.5 s loop. Its
catalog gain is 0.03. It is an optional non-positional ambient cue available
to map authors and the editor sound picker. The Quiet Collection’s Office uses
it on the middle/basement floors, separately from its retained upper-floor HVAC bed.
See the audio guide for an emitter example and the source hash/recipe.
