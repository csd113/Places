# Native audio diagnostics

`places-audio-audit` is an explicit developer tool. It is never an ordinary
Rust test or CI dependency, and it never bakes a map. Build it with the same
locked release features on both sides of a comparison:

```text
cargo build --locked --release --all-features --bin places-audio-audit
places-audio-audit <asset-root> <new-output-directory> [mode]
```

Enable `PLACES_AUDIO_CAPTURE=1` for the bounded internal stereo PCM tap and
`PLACES_BENCH=1` for callback clocks. `PLACES_VERBOSE=1` records the actual SDL
driver, device format and buffer frames. The tap records the game's mixed
output, never a microphone. Output directories must be new.

| Mode | Purpose |
| --- | --- |
| `--tall-guest-cues` | Sequential isolated 23-original-cue bank: front/rear/closed/muted stereo and HRTF, natural tails and two 12.3 s pursuit loops; never layer reference composites with runtime stems |
| omitted | Existing 32-cycle cue/mode/control/latency campaign |
| `--owner-probe` | Fixed-storage Rust allocation census across decode warmup, native installation, scene, voice, cancel/error and unload phases |
| `--disposal-probe` | Movement followed by no updates, bounded query recovery, clear followed by no updates, then restart |
| `--scene-probe` | Animated doors and scripted moving sources in an actual authored acoustic scene; set `PLACES_AUDIO_AUDIT_LEVEL` to its JSON source |
| `--control-probe` | Existing latest-value mute/control pressure witness |
| `--lifetime-probe` | Existing 48 joined preparation/device reloads and pending-start retirement pressure |
| `--sustained-probe` | Continuous 48-voice stereo and HRTF playback; `PLACES_AUDIO_AUDIT_SECONDS` is 1..3600 per mode, default 300 |
| `--fallback-probe` | Record disabled, invalid-content or unavailable-output fallback and unload |

The owner census has 16,384 atomic slots (384 KiB static storage on 64-bit
targets), no callback locks or I/O, and an overflow counter that must remain
zero. It is active only in owner mode. Subtract each process's recorded harness
baseline. A retained warmup allocation is not by itself a leak. SDL's allocation
count of `-1` means this SDL build disables that counter; it never means zero.
Rust allocator bytes exclude native SDL/CRT/driver allocations and process
reservations. Keep process RSS/private observations separate.

`callback_wall_nanos`, calls, maximum and the fixed noncumulative histogram cover
control/admission, renderer, room, sample checks/tap and SDL submission. Separate
admission, renderer, room and submission counters retain those boundaries.
Optional clock/counter overhead is included, except final timing bookkeeping;
the first pointer/argument checks precede the clock. SDL conversion, transport,
driver scheduling and the physical DAC remain outside this callback. Renderer
time includes Kira source/mix work and per-voice FIR/IIR; it does not isolate
those components. Histogram bins are <=0.25/0.5/1/2/4/8/16 ms and >16 ms.

Use matched settings/assets/features and alternating quiet processes for timing.
Keep rejected preflights, raw PCM/counters, source/binary hashes and process
samples. These observations establish neither an OS deadline nor physical
latency, subjective quality/localization, or cross-platform acceptance.

The Tall Guest mode requires its explicit isolated bank with exactly 23 `tg_` cues. It protects native cue admission, directional/occluded signal, mute, natural voice retirement and repeat-loop continuity beyond offline WAV checks. Measured cost about eight minutes (472 s for 186 trials) and under 200 MiB temporary PCM; no map bake or default test/CI execution. Receipts distinguish device PCM from physical localization and subjective listening.
