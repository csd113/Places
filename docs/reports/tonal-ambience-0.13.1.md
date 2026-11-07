# Original tonal ambience integration, 2026-11-07

Three opt-in choices use the existing catalog, editor picker, emitters, ambience
bus and native Kira/SDL3 playback. No authored map selection was changed. No audio
engine cap, voice limit, spatial lifetime rule or renderer behavior was changed.

## Formats and preservation

The fresh starting bank at `3972f6836a08a8448c89858d3f65d28c15188673` had 50 cues,
88 unique WAVs, 92 references and 66,863,584 decoded bytes. All actual WAV headers
were checked, not merely provenance. The favorite original's SHA-256 and Git blob
matched the immutable preparation reference exactly before conversion.

Three 16-second mono PCM16 files at 16 kHz each add 2,048,000 decoded bytes.
The narrow-band favorite's reversible 16 kHz runtime copy saves 6,631,600 bytes.
The final bank has 53 cues, 91 files, 95 references, 16,598,000 disk bytes and
66,375,984 decoded bytes, leaving 732,880 bytes below the unchanged 67,108,864 cap.

The exact 44.1 kHz Empty Room original and all three 48 kHz PCM24 masters are
preserved under `assets/references/audio/tonal_ambient/`, outside runtime export.
All other pre-existing WAVs, gains and map bytes remain unchanged. No claim is
made that compression lowers decoded residency.

The NumPy-only canonical renderer reproduces the four prepared compact hashes
and all preserved master bytes exactly. Explicit per-file rates/pinned hashes,
source dispatch, actual-header provenance and fail-before-publication bank checks
prevent a later import silently restoring an oversized 44.1 kHz bank.
`--import --originals-only` verifies/retains existing recordings without downloads.
Four tiny Python regressions protect rate/hash fallback, cap admission, partial
publication after renderer failure and unknown original-source dispatch. They use
64-frame fixtures/metadata, no maps, devices or substantial asset regeneration.

## Objective evidence and limits

The complete new runtime files have RMS −29.37 dBFS, true peaks −22.3 to −23.3 dBFS
and wrap steps 0/1/1 PCM16 LSB. Their 16 kHz reconstructions have 66.95–66.97 dB SNR
against the high-quality masters. Analytic periodic synthesis and natural-extremum
rotation avoid a pasted splice or recurring fade-to-zero.

The favorite reference has 99.9% of energy below 329.763 Hz and 99.99% below
433.356 Hz. Matched ideal 48 kHz reconstruction of its rate trial changes full-file
RMS −0.0000103 dB, main tone bands less than 0.000019 dB, with −94.695 dBFS RMS error
and 66.388 dB SNR. Displayed integrated loudness/LRA/true peak match at
−27.2 LUFS / 20.3 LU / −15.5 dBFS. Matched wrap steps are −84.26 dBFS trial versus
−84.91 dBFS original, neither an internal-step outlier. The near-silent high-band
noise floor changes with resampling/dither; the original bytes are retained.

These are whole-file numerical comparisons, not a claim that the sound was heard
or perceptually approved. Tool discovery exposed no supported audio-listening or
model-audio-input capability. Native device/mixer coverage and final gate results
are recorded below; human/perceptual listening remains unperformed.

## Real-device native capture

The command sandbox exposed no audio devices and the diagnostic correctly refused
its silent fallback. Supported per-command escalation exposed the existing default
MacBook Pro speakers at 48 kHz, without changing any settings or installing a driver.
The same bounded diagnostic then completed on the real SDL3/Kira device. The empty
output directory left by the refused sandbox attempt was preserved separately.
No dummy driver or silent-output acceptance was used.

`places-audio-audit assets <new-output> --ambient-tones` exercised the normal
catalog choice IDs and native play requests: at least two complete 29.5-second
favorite periods and two complete 16-second periods of every new cue. Actual
loaded state was 91 clips and 66,375,984 decoded bytes, with zero errors and no
dropped capture frames. Complete stereo float32 48 kHz post-mixer files were
analyzed, not short excerpts. Repeated interiors after startup were sample-identical;
wrap steps were below the internal-step 99.9th percentile, with no numerical click
outlier. All captures were finite and unclipped.

An isolated bank containing the exact original 44.1 kHz favorite was captured with
`--ambient-reference`. Across 58.797 seconds after integral onset alignment, the
current compact version versus original had 66.992 dB SNR, −0.00002134 dB RMS
level delta and less than 0.000049 dB change in each main tone band. No gain
normalization, EQ or perceptual claim was used. This directly checks the actual
engine resampler/mixer, separately from the ideal reconstruction evidence.

Mute and master-zero produced exact zero. Positional speaker and headphone
overrides produced finite nonzero output through the normal mono-cue path. A
16.8-second default-gain four-bed mix retained all four owned voices and peaked at
−39.42 dBFS. Clear/unload left zero active/pending voices, loaded clips and decoded
bytes. These are narrowly scoped playback, loop, control and headroom witnesses,
not a general audio-engine or frame-time benchmark.

No tool supplied model audio input, so neither the complete candidates nor the
reference were heard by the assistant. Human/perceptual review and Windows-specific
output validation are separate, still-unperformed checks, not passed gates here.

## Final code and asset gates

Passed against the final implementation:

- `cargo fmt --all --check`
- `cargo check --locked --workspace --all-targets --all-features`
- Debug and release `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `cargo test --locked --workspace`: 529 meaningful tests, zero failures/ignored
- `cargo build --locked --release --workspace --all-features`: all 14 executables
- Production Luau refusal verifier and final Rust test-list reconciliation
- Full Python suite: 377 tests, 3 skips, no failures, with the canonical
  `PLACES_ASSET_ROOT`, explicit release executables and `PLACES_SKIP_SMOKE=1`
- Asset validator (17 pre-existing geometry warnings), audio integrity/budget
  audit, current sound fixture generator `--check`, exact master regeneration and
  byte-identical repeated `--import --originals-only`
- Both bounded real-device capture modes and full captured-file analysis above

The first Python invocation lacked `PLACES_ASSET_ROOT` for its isolated release
binary and failed one geometry-repair test; the corrected complete invocation
passed. The optional legacy `tools/levels/soundscapes.py --check` still refers to
retired `levels/office_demo.json` and fails on that pre-existing missing source; it is not
a current-map audio gate. No retired map was recreated to hide that failure.
There was no unrelated renderer/performance campaign and no CI wait.

Private raw logs, complete captures, source recipes, prepared/reference evidence
and preserved release executables are retained outside the isolated Cargo target
in `Documents/Codex/2026-11-07/ambient-loop-integration/`.
