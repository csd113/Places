# Audio assets and examples

`assets/audio/catalog.json` is the sound catalog. Audio is independent of the
visual `assets/catalog.json`: maps name sound cue IDs, and sound cues name real
WAV files under `assets/audio/`. Every shipped WAV is mono, 44,100 Hz, signed
16-bit PCM. No microphone input or runtime sound synthesis is involved.

The library has **24 cues / 63 WAV files / 5,519,032 bytes** (5.26 MiB). Its
decoded stereo float allocation estimate is **21.04 MiB**, below the runtime's
64 MiB library limit. One-shots range from 0.103 to 1.413 seconds. The longest
loop is 26 seconds. Mono ambient recordings deliberately play without positional
panning; per-cue and per-emitter gains keep the backgrounds quiet.

## Sources and redistribution

Every selected source explicitly declares **CC0-1.0** on its linked primary
page. These declarations were checked on October 16, 2026. CC0's legal reference
is [Creative Commons Zero 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
The original Kenney notice wording is retained under `assets/audio/licenses/`,
with line endings and trailing whitespace normalized for the repository.

| Source and author | Selected content |
| --- | --- |
| [Impact Sounds 1.0 — Kenney](https://kenney.nl/assets/impact-sounds) | Concrete, timber, carpet, grass and snow footsteps; generic, timber, metal, glass, soft, plate and bell impacts |
| [Interface Sounds 1.0 — Kenney](https://kenney.nl/assets/interface-sounds) | Three switch takes |
| [RPG Audio 1.0 — Kenney](https://kenney.nl/assets/rpg-audio) | Two door-opening and two door-closing takes |
| [100 CC0 SFX #2 — rubberduck](https://opengameart.org/content/100-cc0-sfx-2) | Three wet footsteps and one flowing-water loop |
| [202 More Sound Effects — OwlishMedia](https://opengameart.org/content/202-more-sound-effects) | Three short plastic recordings |
| [Different steps on wood, stone, leaves, gravel and mud — TinyWorlds](https://opengameart.org/content/different-steps-on-wood-stone-leaves-gravel-and-mud) | One gravel take; the author identifies edited pdsounds.org recordings in the page comments |
| [Water Splash and sand footsteps — Peludo](https://opengameart.org/content/water-splash-and-sand-footsteps) | Four isolated steps from the submitted sand recording. The author's requested itch.io reference is [RNAn](https://rnan.itch.io/), linked from their OpenGameArt profile |
| [Airvent Loop — Longor](https://opengameart.org/content/airvent-loop) | A recorded home air vent, crossfaded into a quiet HVAC loop |
| [Ambient Bird Sounds — isaiah658](https://opengameart.org/content/ambient-bird-sounds) | A cleaned outdoor camera recording, shortened and crossfaded for the garden |
| [rubber duck -CsG-.wav — csaszi](https://freesound.org/people/csaszi/sounds/350917/) | Two isolated squeaks from the publicly offered HQ MP3 preview; the login-only original WAV was not downloaded |

`assets/audio/provenance.json` is the exact per-file mapping: destination WAV,
source id, original archive member/file name, original member SHA-256 and final
WAV SHA-256. Its source table retains the actual download URL and download/archive
SHA-256. `authoring.json` records every trim in seconds, crossfade length and
integer peak target, so the changes are inspectable without the original packs.
The source archives remain outside the repository; only selected runtime assets
and their provenance ship. No unrelated pack previews or third-party artwork ship.

## Cue choices

| Cue | Takes | Intended use |
| --- | --- | --- |
| `footstep_hard` | 4 | Concrete, ceramic tile and other hard floors |
| `footstep_wood` | 4 | Timber floors and steps |
| `footstep_carpet` | 4 | Carpet |
| `footstep_grass` | 4 | Vegetation and soft ground |
| `footstep_gravel` | 1 | Loose gravel; this source has one take |
| `footstep_sand` | 4 | Sand |
| `footstep_snow` | 4 | Snow |
| `footstep_water` | 3 | Wading splashes |
| `switch_click` | 3 | Mechanical switch feedback |
| `door_open`, `door_close` | 2 each | Interior door movement |
| `impact_hard`, `impact_wood`, `impact_metal`, `impact_glass`, `impact_soft` | 3 each | Restrained physical contact feedback |
| `prop_plastic` | 3 | Small plastic object taps/rustles |
| `prop_ceramic` | 3 | Light plate clinks, selected for the tappable mugs |
| `prop_bell` | 2 | A short bell ring |
| `prop_duck` | 2 | Actual rubber duck squeaks |
| `ambience_hvac` | 1 / 7.239 s | Quiet room ventilation |
| `ambience_outdoors` | 1 / 26.000 s | Daytime garden birds |
| `ambience_water` | 1 / 6.167 s | Flowing water in the pool hall |
| `water_emitter` | Shares `ambience_water` WAV | Positional world-time water source for native distance/pan checks; no extra WAV or decoded allocation |

Source files were decoded once offline, downmixed/resampled to the fixed WAV
format, DC-corrected, and normalized to a bounded integer peak. One-shots trim
near-silence, retain a short lead/tail and use 3 ms edge fades. Ambient selections
overlap their original tail and head using the explicit linear crossfade recipe;
they do not fade to silence each lap. Gains in the runtime catalog are separate
from peak normalization. Import never manufactures additional takes or substitutes
one material's recording while describing it as another.

## Maintained environment demonstrations

`tools/levels/soundscapes.py` applies the following additions idempotently.
The Office and Aero generators, and Pool refinement tool, call the same helper
so regeneration preserves their audio. Home's standalone source is maintained
directly; the helper restores its mug interaction when requested.

| Map | Authored behavior |
| --- | --- |
| Office: After Hours | The existing upper vending machine owns a non-positional, real-time HVAC emitter. An aggregate player region covers only the upper floor/vestibule and stops it on descent. It starts playing because the authored spawn is already inside. The existing top-floor mug supports E → `play_sound`, with a 0.45 s cooldown |
| Pool Showcase | The existing water-demo control owns quiet world-time water ambience. The pool-hall player region stops it outside the hall and restarts on entry. The spawn is inside, so the authored initial emitter state is playing |
| Home Showcase | The existing coffee-table mug supports E → `play_sound`, with a 0.45 s cooldown and a measured aim box |
| Frutiger Aero | The existing terrace bench owns non-positional birds. The garden region starts/stops the loop on player entry/exit, leaving the indoor route quiet |

These additions do not move props, lights, walls, water or floors. A prepared map
must be recompiled to include changed semantics; the player continues to load
prepared packages. Full-map preparation remains an explicit developer action.

## Small native acceptance fixture

`tests/fixtures/native/sound.level.json` is an opt-in two-room fixture derived
from the established developer-console fixture. It is outside the shipped lineup
and never bakes as part of Rust tests or automatic CI. Its generator is
`tools/levels/build_sound_fixture.py`.

- `fixture_door` fills the common wall's real aperture; `fixture_switch` rings a
  bell and switches the light, with a 0.6 s interaction cooldown.
- `native_mug` taps ceramic with the same cooldown contract as the real maps.
- `native_duck` and `other_duck` are distinct pickup-capable instances of the
  same rubber duck asset, on the existing table.
- `audio_exact_held_duck` ANDs the duck asset, exact `native_duck` instance and
  `held: true`. Its box is X −1..1, Z 0.1..1.1, Y 0..2.5; it squeaks only for
  the selected held instance.
- `audio_duck_type_unheld` accepts either duck asset instance while unheld,
  in the east room's northern strip, and rings a bell with a 0.6 s cooldown.
- `audio_aggregate_region` accepts player and physical items. The first entry
  starts the real-time HVAC loop on `fixture_reset`; the final exit stops it.
  Its box is X 4..8, Z −0.5..2.5, Y 0..2.5.
- `audio_thin_once` is a 3 cm player-only strip just beyond the door, ringing
  once per run. Crossing it without ending a frame inside exercises swept entry.
- `fixture_activate` starts its spatial world-time `water_emitter` loop at
  X −2.95, Y 1.2, Z 0.8. Moving around it checks distance and stereo pan;
  `fixture_reset` stops both loops. Changing simulation time can compare the
  world and real domains. Regional ambience retains its non-positional cues.
- The east room has carpet, hard tile and gravel strips; the west floor is timber.

The generator/source audit does not establish native audible playback, door swing
or prepared package acceptance. Those require the explicit compiler/native run.

`tests/fixtures/native/sound-stress.level.json` is a second explicit native source,
with id `sound_stress_native`, using the same two-room geometry. Sixty buried,
non-solid, non-occluding source carriers start with `ambience_hvac`, gain 0.03,
looping enabled and playing disabled. `fixture_activate` starts all sixty;
`fixture_reset` stops all sixty. Each control uses eight interact bindings
(seven batches of eight actions and one of four), preserving the existing eight
actions per binding and sixteen bindings per entity limits. Ordinary region
triggers are omitted so the controls determine the active stress voices. Repeated
activate/reset cycles exercise native voice saturation, pending work and cleanup.
This source is never automatically baked; the same generator `--check` verifies
both fixtures.

## Authoring and checks

From the repository root:

```sh
python3 tools/audio/build.py --check
python3 tools/levels/soundscapes.py --check
python3 tools/levels/build_sound_fixture.py --check
python3 tools/levels/build_office.py --check
python3 tools/levels/build_frutiger_aero.py --check
python3 tools/assets/validate.py
```

The audio check is offline and uses only Python's standard library. It verifies
all destination hashes and source mappings, WAV format, useful non-clipped signal,
loop-boundary derivative, per-file/library budgets, orphan WAVs and audio cue
references in maintained maps/native fixtures. It never downloads, decodes source
archives, opens an audio device or bakes a map.

To deliberately regenerate the WAVs, install `ffmpeg` and place the pinned source
downloads at the cache filenames recorded in `authoring.json`, then run:

```sh
python3 tools/audio/build.py --import --cache /tmp/places-audio-sources
# Only if the pinned sources need downloading:
python3 tools/audio/build.py --import --cache /tmp/places-audio-sources --download
```

Downloads are hash-checked before use and ZIP members are read directly without
extracting paths. A changed upstream hash is a review failure, never an automatic
update. The importer records the `ffmpeg` version; decoder/resampler version
changes may alter exact PCM bytes, so review and retain the new output hashes
deliberately. The committed WAV bytes are the runtime authority.
