# Menu redesign verification

Places keeps its bitmap font, warm dark panels and yellow selection accent. A shared layout now draws each row and supplies its pointer region. Titles, short section descriptions, aligned values and one contextual hint replace the dense centred lists. Maps and Controls scroll within bounded lists; Import and Back stay visible. No existing option was removed.

| Native screen | Before | After |
| --- | --- | --- |
| Main | [Capture](../images/menus/before-main.png) | [Capture](../images/menus/after-main.png) |
| Map selection | Archived locally | [Capture](../images/menus/after-maps.png) |
| Graphics | [Capture](../images/menus/before-graphics.png) | [Capture](../images/menus/after-graphics.png) |
| Controls | Archived locally | [Capture](../images/menus/after-controls.png) |

These are genuine native screenshots at 960×544 logical / 1920×1088 drawable, including the macOS title bar. The capture tool supplied JPEG frames; these were saved as real PNGs with identical decoded pixels, without resizing or retouching. Baseline: `324d02265c36e1987d0def6a03c13ccb93fff4be`. Additional archived views cover Pause, independent lighting preferences, loading, failed-load recovery and 640×360. Native 1280×720 windowed and desktop fullscreen were also exercised; on the 1512×982 display, SDL reported a 1512×949 fullscreen content area below the macOS menu bar. The available display uses 2× backing scale; deterministic tests cover 1× and 1.5× conversions, letterboxing and invalid sizes.

Pointer motion selects a row. A fresh left press activates once; release is required before another action. Value arrows step backward/forward; clicking the value or row advances. Keyboard focus persists until real pointer movement. Menu, scroll, modal, focus and viewport changes discard stale hover. Loading and binding prompts expose only Cancel. Menus show the cursor; focused gameplay captures it.

Native checks covered nested Escape returns, repeated pause/resume, all 17 control rows, rebinding/cancel, scrolling, expanded Graphics, disabled lighting/resolution feedback, live texture and lighting changes, load cancellation and an invalid isolated map package. While paused, 470 initial state samples and a separate 1,112-sample pointer verification interval retained identical position/yaw/pitch across navigation and focus switches; resuming restored movement and mouse-look. Restore Defaults remains the existing immediate action; no confirmation dialog or controller navigation exists in this source.

Validation passed: formatting, strict debug and release Clippy, 2,253 all-feature unit tests plus seven integration tests (32 ignored), 37 focused UI tests, and both existing native Low-lighting GPU gates. Final modal/footer presentation was followed by focused tests, both Clippy profiles and a release rebuild. Fixed-spawn High and Low gameplay captures were pixel-identical to baseline. No map, texture or renderer resource pipeline changed; no meaningful performance delta is claimed.

The computer-use tool sometimes supplied stale pointer coordinates. With explicit user approval, a macOS mouse-event helper restricted to the frontmost Places test window completed real pointer verification: hover and keyboard handoff, fresh-press activation, held-button protection across opening a menu, outside clicks, value arrows, disabled feedback, binding Cancel, scrolling, map selection/loading Cancel, pause navigation and resumed mouse-look. Pointer targets were exercised at 960×544, after resizing to 1280×720, in fullscreen and after returning to windowed mode. Clicks in the fullscreen letterbox did nothing. No macOS security settings changed.

Raw logs, traces, isolated settings and fixtures remain under `tools/bench/results/menus-20261011/` (ignored). No user settings or repository maps were modified. Exact-commit CI and custody are recorded in its final handoff.
