# Audio assets and examples

`assets/audio/catalog.json` is the sound catalog. Audio is independent of the
visual `assets/catalog.json`: maps name sound cue IDs, and sound cues name real
WAV files under `assets/audio/`. Every shipped WAV is mono signed 16-bit PCM. Most use 44,100 Hz; the
three tonal loops and narrow-band Empty Room runtime copy explicitly use
16,000 Hz. Archival masters are outside the runtime bank. No microphone input or runtime sound synthesis is involved.

The current library has **53 cues / 91 unique WAV files / 95 file references /
16,598,000 disk bytes**. Its retained stereo-float allocation is **66,375,984
bytes**, leaving **732,880 bytes** under the unchanged 64 MiB limit. Each unique
source frame costs eight decoded bytes, even for a mono/low-bit-rate file;
compression alone would not lower this residency. The native loader accepts
PCM16 at 8–48 kHz, up to 30 seconds per clip. Per-file declarations and actual
headers must agree. Shared canonical paths decode only once.

## Sources and redistribution

Imported recordings explicitly declare **CC0-1.0** on their linked primary
pages. Original Places interior ambience and elevator cues are MIT-licensed
offline synthesis, with recipes retained under `tools/audio/`. The original imports were checked on October 16, 2026; the intercom addition was checked on November 5, 2026. CC0's legal reference
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

[The intercom of nothingness — GrimmReaperSounds](https://freesound.org/people/GrimmReaperSounds/sounds/560697/)
was added under CC0 on November 5, 2026, using the exact user-supplied
`560697__grimmreapersounds__the-intercom-of-nothingness.mp3` (792,259 bytes).
Its full source is approximately 85.746 s. The runtime keeps the existing
30 s per-clip limit: seconds 10–40 become a 29.5 s loop after a 0.5 s
tail/head crossfade. The complete attachment remains the hash-pinned offline
source; no preview or replacement was downloaded. Courtesy credit goes to
GrimmReaperSounds. This file has its own decoder version in provenance.

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
| `ambience_intercom` | 1 / 29.500 s | Optional quiet, non-positional intercom bed by GrimmReaperSounds (CC0) |
| `water_emitter` | Shares `ambience_water` WAV | Positional world-time water source for native distance/pan checks; no extra WAV or decoded allocation |

Source recordings were decoded once offline, downmixed/resampled to their declared
mono PCM16 runtime format, DC-corrected, and normalized to a bounded integer peak. One-shots trim
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
| Office: After Hours, lower floors | One sound-only `office_intercom_bed` plays `ambience_intercom` on the middle/basement floors through `audio_office_lower`. It stops before the existing upper-floor HVAC region, retaining that track and avoiding two non-positional beds. The normal upper-floor spawn leaves the intercom stopped until descent |
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
python3 tools/levels/build_sound_fixture.py --check
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

### 0.13.0 polish

The library now has **25 cues / 63 WAVs**. `water_transition` reuses the three
recorded wet splashes for a restrained swim entry/exit cue; loading/reset does not
manufacture a transition. A 750 ms real-time retrigger guard prevents shallow
ramp contact from producing repeated splashes; it changes no water physics and
queues no delayed sound. Unload resets both the state and guard.
The unusually loud third switch/plastic takes receive
explicit post-normalization gains of 0.65 / 0.5. Source identity, trim and edge
fades are unchanged; `authoring.json` and `provenance.json` retain the processing
and hashes, and `tools/audio/build.py` reproduces/audits the gain step offline.
The audit now includes maintained project levels and standalone emitters.

Standalone emitters may author `attenuation: [near, far]` with metre distances,
`0 <= near < far <= 1000`; omission uses the cue. Editor Near/Far controls,
Gain and Stop preview share the existing source/mixer lifecycle. Persistent
`Settings::master_volume`, `set_master_volume` and `AudioSettings::master_volume`
already cover active and future gameplay sounds, effects and ambience. Zero is
silent without discarding loops. The following menu stage should reuse that
setter and normal settings persistence rather than introduce another master bus.

The `elevator_shaft` and `elevator_arrive` positional cues are original committed
WAVs rendered offline by `tools/audio/elevator.py`. Their exact output hashes,
PCM format and recipes are tracked in the ordinary authoring/provenance records.
Lift runtime uses 1.5 m near / 12 m far attenuation and occasional shaft passes;
see [passenger elevators](../ELEVATORS.md).

`elevator_travel` is an original 1.8-second mono traction-motor rise and release,
rendered offline by the same tool. It plays once at the passenger's ears during
the sealed simulated ride, before either local transfer or level preparation.
Its source anchor remains live until that interval ends; ordinary closed doors
and idle cabins do not play it. The catalog applies the normal effects volume.

### Optional intercom ambience

`ambience_intercom` appears in the ordinary editor sound picker. Map authors
can opt in with a `sound_emitters[]` entry such as:

```json
{ "id": "intercom_bed", "x": 0, "y": 1, "z": 0,
  "sound": "ambience_intercom", "gain": 1, "spatial": false,
  "looped": true, "enabled": true, "playing": true, "time_domain": "real" }
```

Catalog gain 0.03 keeps the layer quiet before the ordinary master and
ambience controls. One emitter is sufficient for a non-positional bed; avoid
stacking copies. The maintained Office map uses this cue on its middle and
basement floors. Its existing upper-floor HVAC remains. Movement cues and hearing
behavior are unchanged. The measured file and loop checks establish technical
playback suitability; subjective listening and cue masking need a separate
listening review.

To regenerate, supply that exact MP3 under its recorded `cache_file` name in
the import cache alongside the other pinned sources. The importer refuses
a missing attachment even with `--download`, preventing site substitution.

The playable assignment is `projects/environments/levels/office.json` in The Quiet
Collection. It uses one regional emitter at gain 1 before catalog gain 0.03. The
player region spans X −8..36, Z 0..18 and Y −0.1..6.9; the existing upper HVAC
region starts at Y 7.0. Crossing the regions stops one bed before starting the
other. Map unload retires the source through ordinary audio ownership. The
current 29.5 s WAV and original attachment/provenance are unchanged.

## Optional lonely tonal interiors (0.13.1)

The normal editor sound picker now includes `threshold_air`, `pale_signal` and
`below_the_landing`. They are original 16-second single-pitch beds with very slow
breathing, faint sympathetic partials/air and circular room processing, not
music tracks, recordings or runtime synthesis. Catalog gain 0.065, looping enabled
and non-positional defaults keep their setup consistent with quiet interiors.
Spatial emitter overrides remain available because the files are mono. No map
selection, including the deferred duck map, was changed.

Choose a cue in a Sound Emitter or regional ambience component, or author:

```json
{ "id": "quiet_tone", "x": 0, "y": 1, "z": 0,
  "sound": "threshold_air", "gain": 1, "spatial": false,
  "looped": true, "enabled": true, "playing": true, "time_domain": "real" }
```

Use a single bed rather than stacked copies. Threshold Air centers at 196 Hz,
Pale Signal at 246.9375 Hz and Below the Landing at 164.8125 Hz. The new dry files
measure about −29.37 dBFS RMS, −29.8 to −30.3 LUFS and −22.3 to −23.3 dBFS true peak.
These objective measurements are not a subjective listening review.

`assets/references/audio/tonal_ambient/` preserves three exact 48 kHz PCM24 masters
and the unchanged 44.1 kHz favorite Empty Room original. They are authoring
references, not catalog members or runtime-export inputs. Its manifest pins their
hashes. `tools/audio/tonal.py` regenerates all three original masters and the
compact runtime variants using NumPy only; no SciPy, sample library or service is
needed. NumPy is an offline authoring dependency, never a game dependency.

The favorite original source SHA-256 remains
`f13f2c0cf3181d12799cf2ee5014e9822ee348c07c4343ac64bd235dede08059`.
Its 29.5-second 16 kHz runtime copy has SHA-256
`ef7174a59fdd5e67ecfc06d5c8fd65b44221cac6bf0f48fba3ec5dac295059b6`.
The original renderer and its source identity remain intact in provenance. The
conversion is reversible from the saved original or `original.render("empty_room")`.
99.9% of original energy is below 330 Hz; matched ideal 48 kHz reconstruction
changes its RMS by −0.0000103 dB with 66.39 dB SNR. It preserves the displayed
−27.2 LUFS / 20.3 LU LRA / −15.5 dBFS true peak. These numerical comparisons do
not substitute for perceptual listening or establish arbitrary low-rate quality.

### Reproducible per-file format policy

`authoring.json.sample_rate` is the legacy default, not an all-files mandate.
The four compact clips explicitly declare `sample_rate: 16000` and a pinned
`expected_sha256`. Empty Room additionally pins the original source hash,
`periodic_fft_tpdf` conversion and dither seed 72199. The tonal source has an
explicit renderer dispatcher; unknown original source IDs fail closed.

Import records each actual WAV's rate, frames, channels and bit depth. Audit
checks those against the recipe, duration against the actual rate, exact hashes,
loop derivatives and the unchanged bank cap. Every import renders/stages the
whole selected bank and checks its decoded budget before publishing any asset
or provenance. A missing compact-rate override fails format/hash validation
rather than silently publishing an oversized 44.1 kHz bank.

To regenerate only original cues while verifying and preserving the recordings:

```sh
python3 tools/audio/build.py --import --originals-only
python3 tools/audio/build.py --check
```

This does not download or require third-party source caches. Full `--import`
continues to require the exact pinned source archives/attachments. Neither mode
changes the other cues' bytes, gain or lifecycle policy. Inspect the
[dated integration report](../reports/tonal-ambience-0.13.1.md) for actual native
capture coverage and listening limits.
