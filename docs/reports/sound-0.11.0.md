# Sound engine and content — 0.11.0 staging

Implementation for [issue #12](https://github.com/csd113/Places/issues/12), based on
`9e1bb32068c14311341d9cd20e3833fba86c5bee`, on `0.11.0-staging`. The editor remains
deferred. This stage does not change the renderer, light placement or caustics.

## Ownership and bounded work

`engine::audio` owns a [Kira 0.12.5](https://docs.rs/kira/latest/kira/) mixer
through a small [SDL3 playback backend](https://wiki.libsdl.org/SDL3/SDL_OpenAudioDeviceStream).
Kira remains the maintained mixing solution; using the existing SDL3 device layer
avoids CPAL's additional device-management thread and its disconnect panic path.
The backend retains the [logical default output device](https://wiki.libsdl.org/SDL3/SDL_OpenAudioDevice),
so SDL handles device changes. Initialization failure produces a silent owner, and browser builds keep
the same silent API. No microphone or recording device is opened.

The catalog admits at most 64 cues, 128 file references, 16 MiB per WAV, 30 seconds
per clip and 64 MiB decoded samples. All selected sounds are decoded and shared
before gameplay. Forty-eight mixer voices and a combined 48 live/pending source
budget bound playback; stopped-loop replacement can retry without unbounded
queues. One-shots drop on overload. Map replacement clears pending starts and
stops live voices; shutdown closes the SDL stream before freeing callback data.
Ordinary playback performs no file I/O or decoding. The callback generates PCM
into fixed scratch storage; SDL owns its own buffering and device thread.

A nonblocking atomic gate excludes Kira's resource-publication and retirement
boundaries, preventing an upstream retired-queue interleaving identified during
independent review. Contention never waits: existing audio continues processing,
loop starts retry within the budget and one-shots may drop. No Tokio runtime or
second preparation pool was added. Explicit benchmark capture alone enables a
bounded output PCM tap, drained and written by the game thread.

`game::app::audio_runtime` selects material footsteps, switch feedback, actual door
motion, selected prop interactions and physical impact cues. Physics exposes
bounded contact onsets and retained substep paths; it does not own playback.
Impact minimum speed/impulse, per-item cooldown and a global token budget suppress
resting contact chatter. Player footsteps follow walked distance, with teleport,
climbing, swimming and airborne guards. Spatial sources follow live physical
body centres. Stereo pan and smooth distance falloff are used; HRTF, acoustic
occlusion and reverb are outside this stage.

The listener remains at the controlled player's ears in third person and after
`/become`. Menus, console, loading and focus loss pause both clock domains.
World-domain speed follows `/time scale` exactly, including zero; real-domain
player actions and authored ventilation retain normal speed at zero world time.
Player movement and held-object control retain their existing unscaled rules.
Master, effects and ambience volume plus mute persist under Settings → Audio.
A deliberately stopped loop fades for 12 ms; that short retiring tail can finish
across a subsequent reset/mute boundary.

## Shared authoring and sounds

Audio components/actions use the existing typed entity infrastructure. Trigger
filters select player, any actor, or an exact item asset/instance and held state;
selectors combine. Per-actor and aggregate occupancy support enter/exit, once,
repeat and cooldown bindings. Swept player and physical substep paths catch thin
crossings, including bounced paths and carried items. Accepted events defer
across the per-frame budget rather than discarding cleanup exits. Full-queue
membership rollback retries final exits. Synchronous event descendants preserve
the chain-depth limit across deferred frames.

The library contains **24 cues, 63 real WAV files, 5.26 MiB on disk and 21.04 MiB
decoded**. Selected CC0 recordings come from Kenney, rubberduck, OwlishMedia,
TinyWorlds, Peludo, Longor, isaiah658 and csaszi. Exact primary declarations,
source/archive/member hashes, import edits and output hashes are recorded in
[the asset guide](../assets/AUDIO.md), `assets/audio/provenance.json`,
`authoring.json` and retained notices. The duck uses the actual public HQ MP3
preview, rather than claiming access to the login-only original WAV.

Office has a quiet upper-floor HVAC region and tappable mug; Pool has hall water
ambience; Aero's outdoor region has birds; Home has a tappable mug. Generator
integration preserves these additions. Prepared maps require recompilation.
The small native fixtures stay outside the shipped lineup and automatic CI.
Office audio evidence uses a verified current Static package; the small native
fixture also exercises Full playback. Full Office lighting rebaking is outside
this audio acceptance campaign.

## Validation and evidence

Local formatting, locked all-target/all-feature check, strict debug and release
Clippy, the all-feature release workspace build and all 26 Rust tests pass.
Nineteen focused audio/trigger/contact regressions add about 0.01 s CPU runtime:
they protect allocation and path limits, callback resource contention, source
cleanup, trigger overflow/chain depth, contact noise and true swept crossings.
They do not bake maps. The existing Python campaign passes 372 tests with four
expected skips; asset, audio, prop, texture and maintained-generator checks pass.
Two independent final reviewers found no remaining confirmed implementation
blocker after the retirement race and event-chain fixes.

Eight direct-executable native campaigns pass on Apple M2 Pro / Metal:

| Campaign | Observed outcome |
| --- | --- |
| Controls | Cooldown/rearmed switches, open/close/reopen door, selected prop cue, exact held instance, type/held rejection, once-only thin crossing |
| Movement | Real W movement selects wood, carpet, tile/hard and gravel; footsteps continue at zero world time and stop in menus |
| Lifecycle | Actual distance attenuation and turn-reversed pan; world/real clock policy, menu/focus pause, `/become` listener invariance, explicit stop and reload |
| Settings | Ordinary keyboard navigation changes all three volumes, toggles mute repeatedly, produces exact PCM silence while muted and preserves choices after reload |
| Unavailable audio | Invalid process-local SDL audio driver reaches the initialization-error fallback; the game continues with zero voices/output frames |
| Voice overload | Ten start/replace/stop cycles of 60 requests retain at most 48 voices; 1,008 accepted starts, 252 overload refusals, zero errors and stable cache |
| Physical contact | Ordinary E pickup/release produces one contact start; the body sleeps and no further sound starts during three seconds at rest |
| Map changes | Ten alternating map changes clear every old source and restart the next; decoded memory stays exactly 22,065,040 bytes and process RSS settles near 335 MiB |

Native captures contain finite, nonzero **48 kHz stereo output PCM**, with zero
capture drops in accepted runs. These observations establish actual callback
output and accepted playback; they do not establish perceived sound quality.
Human listening was a separate acceptance step because this execution
environment cannot listen to its audio input. During final release acceptance,
the user listened to the native clip and answered **“Listened — acceptable”**.
That confirms listening acceptance without claiming physical unplug/replug or
cross-platform device validation. The [fixture clip](../videos/sound-0.11.0-fixture-native.mp4)
contains 360 real native frames and the playback callback's PCM, with no
microphone or substituted soundtrack. Per-frame presentation timestamps follow
the recorded output-audio clock (maximum mux quantization error 0.21 ms).
The [Office clip](../videos/sound-0.11.0-office-native.mp4) contains 300 native
frames with authored ventilation and actual carpet footsteps, using the verified
Static package. Its mux timestamp error is below 0.17 ms.
The [Audio settings capture](../images/sound-0.11.0/audio-muted.png) retains the
unchanged native frame. Capture runs are intentionally paced and excluded from
performance measurements.

The tested all-feature game executable has SHA-256
`2088fc1fe237718556503eaad7d216bbc02830fbf82e4150c1baaa9b17cdb043`.
Raw traces, output WAVs, plans, gate logs, package identities and compact receipts
remain in `local/sound-0.11.0/`, outside Cargo's build directory. The final game
and compiler are preserved once under its `bin/` directory.

## Average FPS

A controlled audio-off/on comparison uses the same final executable, current
Office Static package, stationary camera `(21, 1.6, 10)`, High textures/16×
filtering, Static/low lighting, reflections Off, bloom On and 60° FOV. Each run
warms up 240 frames and measures 3,600 frames; order is off/on/on/off. V-sync and
all frame caps are disabled. The actual mode is **Immediate**, with **1920×1080
physical drawable pixels**. More than 81% of measured frames finish faster than
the display's 120 Hz interval, and sustained throughput exceeds refresh: these
runs are uncapped. No video/PCM capture or competing build runs during timing.

| Playback | First average FPS | Second average FPS | Pooled average FPS |
| --- | ---: | ---: | ---: |
| Disabled | 198.6 | 199.7 | **199.2** |
| Enabled, authored HVAC loop | 195.1 | 207.2 | **201.0** |

The difference lies within run variation; no throughput improvement is claimed.
This measures controlled playback cost in the current build. Earlier pre-stage
and Full-lighting diagnostics showed display pacing and are **inconclusive**;
they are excluded from the matched result. A whole-stage Full-lighting throughput
comparison is therefore not established.

Developer teleports and player-only respawn retain the established trigger
baseline policy: no enter/exit events are fabricated, and existing cue state
is retained. Audition region transitions with normal movement; full reset/load
restores authored emitter state.

Unknown elevated prop/architectural support surfaces currently use the hard-floor
footstep fallback; authored floors, regions, ramps and stairs select their
material. Device disappearance/reconnection follows SDL's logical-device
behavior; physical unplug/replug has not been performed on this host. An initialization
failure stays silent for that session; restart after restoring assets/device
availability. Web audio
is intentionally silent for this initial desktop stage.
