# Windows UI readability - 0.13.1

This local pass replaces ordinary NPC/message overlays with subtitles, reduces
loading presentation, and makes Explore cards easier to read. It follows the
FPS report commit `2f5b7d4c1a232e35a9624e48ec9994fd4b67a853` on `0.13.1-dev`.
The preceding FPS report and evidence are retained unchanged. No rendering
optimization, save/checkpoint implementation or hub sign editor work is included.

## Result

Ordinary `ShowMessage`/`api.message` output uses a bottom-center speaker and up
to two losslessly wrapped rows on a translucent backing. Consecutive different
messages queue; identical adjacent messages coalesce. Each page receives 3.5-9
seconds based on its length. Enter/PgDown advances, PgUp rereads, and Backspace
skips the current message. Movement and ordinary Interact remain available.
The interaction hand and blocked-release hint move above the speech area.
Explicit F1 help retains its dismissible guidance panel. Reading time pauses
with pause, help, focus loss, loading and the console. Cancellation restores
the old conversation; a committed map, replay/reset or run completion clears it.
The existing API has no dialogue-choice UI; no choices were removed or invented.

Startup, level loads, transitions and editor Play share `Loading level...`, a
thin bar and, where cancellation is possible, one cancel affordance. The solid
fill advances monotonically from actual preparation milestones, reaches its
final value only after GPU/game installation, and displays no estimated time
or percentage. A separate traveling marker indicates unknown-duration work;
it never contributes to completed fill. Technical phases/decoded-byte counts
remain in logs/traces. Existing full error pages and recovery behavior remain.
The loader, package format, compiler fingerprint and map contents are unchanged.

Explore retains four visible cards and existing package discovery/actions.
Cards have wider bodies, separate 16:9 previews, padding and gutters, bounded
three-line titles, and ellipsis only when needed. Tab or the details affordance
opens the full name and paged description; closing restores selection. Rendering
and pointer input share the same rectangles and drawable transform. Vertical
navigation reaches the pinned actions on an odd final row. Empty collections,
scrolling, preview fallback, actual previews and explicit selection remain.

## Windows evidence

Final format, strict workspace/all-target/all-feature Clippy, the locked release
workspace/all-feature build, and all 386 Rust tests pass. Three tiny CPU
regressions cover speech order/text retention/repeat/reset, contained card
geometry/full-name detail ownership, and odd/empty grid navigation. Ordinary
Rust tests do not bake maps. All 371 Python source tests pass with 18 expected
skips. The asset validator checks 674 assets with zero warnings. Texture/prop
checks pass with their existing preferred-budget advisories. No visual asset
was created or changed.

The final native cohort has 173 scripted game actions and 35 GPU framebuffer
captures, plus small-fixture editor Play. Assertions cover queued speakers,
long wrapped speech, movement, normal Interact while speaking, repeat, pause,
F1, focus-event timing, cancellation/repeat, conversation restoration and
replacement, startup/transitions, missing/corrupt package failures and dismissal,
empty/seven-package Explore, selection/scrolling and details. Editor Play returns
to editing with source bytes unchanged. Native apps run at Normal priority;
background compilation and the editor compiler child run BelowNormal.

Actual captured drawable sizes are 960x544, 960x600, 1280x720, 1000x900,
1440x900 and 1920x1080. Pixels were inspected, including title/preview containment,
full details, HUD separation, loading and errors. The editor reports Windows
150% display scale; game logical and drawable sizes match in this cohort.
An existing environment package passes final-compiler integrity and
`--require-current` verification without a full-map rebake.

## Limits and custody

The original UI cohort retained about 3.7-4.1 seconds between event pumps before
the first presented loading frame. The [targeted startup follow-up](startup-0.13.1-windows.md)
records the later shader-worker fix, paired measurements and remaining pre-loop
initialization limit. Warm
scripted Escape dispatch (queue to handler) in the controlled cancellation
cases is about 2.5-4.1 ms; injected
worker delays make interruption repeatable, and do not drive progress fill.
These are UI lifecycle observations, not an FPS comparison or a claim that all
startup stalls were removed. Stage completion remains intentionally coarse.

An OS-level mouse click check remains unverified: Windows computer-use capture
returned `FrameArrived timed out`, then `window capture timed out` after a fresh
window selection. No coordinate click was sent. Shared CPU hit-region tests,
keyboard paths and native rendered layouts passed; this is not native cursor
click/scroll proof. Other native DPI scales are unverified. Mac validation is
explicitly deferred; no Mac access was requested or attempted.

Executables/DLLs, recipes, captures, traces, metrics, hashes, source gates and the
receipt are preserved under `local/verification-0.13.1-ui/`, outside `target`.
`validation-receipt.json` names the accepted cases and superseded monitor/recipe
failures; `binary-hashes.json` identifies the executables. Provisional evidence
is retained and excluded from final assertions. Final source identity, cleanup
and idle custody are recorded locally. Main/v0.13.0 remain unchanged. No push,
upload, release, tag or main merge is part of this pass; the parent-managed
remote-write hold remains pending resolving authorization.
