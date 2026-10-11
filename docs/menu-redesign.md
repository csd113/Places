# Menu redesign verification

Places keeps its bitmap font, warm dark panels and yellow selection accent. A shared layout now draws each row and supplies its pointer region. Titles, short section descriptions, aligned values and one contextual hint replace the dense centred lists. Maps and Controls scroll within bounded lists; Import and Back stay visible. No existing option was removed.

| Native screen | Before | After |
| --- | --- | --- |
| Main | [Capture](images/menus/before-main.png) | [Capture](images/menus/after-main.png) |
| Map selection | [Capture](images/menus/before-maps.png) | [Capture](images/menus/after-maps.png) |
| Graphics | [Capture](images/menus/before-graphics.png) | [Capture](images/menus/after-graphics.png) |
| Controls | [Capture](images/menus/before-controls.png) | [Capture](images/menus/after-controls.png) |
| Pause | [Capture](images/menus/before-pause.png) | [Capture](images/menus/after-pause.png) |

These are genuine native screenshots at 960×544 logical / 1920×1088 drawable, including the macOS title bar. Baseline: `324d02265c36e1987d0def6a03c13ccb93fff4be`. Additional views show [independent lighting preferences](images/menus/after-lighting.png), [loading](images/menus/after-loading.png), [failed-load recovery](images/menus/after-failed-load.png) and [640×360](images/menus/after-small-graphics.png). Native 1280×720 windowed and 1512×982 desktop fullscreen were also exercised. The available display uses 2× backing scale; deterministic tests cover 1× and 1.5× conversions, letterboxing and invalid sizes.

Pointer motion selects a row. A fresh left press activates once; release is required before another action. Value arrows step backward/forward; clicking the value or row advances. Keyboard focus persists until real pointer movement. Menu, scroll, modal, focus and viewport changes discard stale hover. Loading and binding prompts expose only Cancel. Menus show the cursor; focused gameplay captures it.

Native checks covered nested Escape returns, repeated pause/resume, all 17 control rows, rebinding/cancel, scrolling, expanded Graphics, disabled lighting/resolution feedback, live texture and lighting changes, load cancellation and an invalid isolated map package. While paused, 470 state samples retained identical position/yaw/pitch across navigation and a focus switch; resuming restored movement. Restore Defaults remains the existing immediate action; no confirmation dialog or controller navigation exists in this source.

Validation passed: formatting, strict debug and release Clippy, 2,253 all-feature unit tests plus seven integration tests (32 ignored), 37 focused UI tests, and both existing native Low-lighting GPU gates. Final modal/footer presentation was followed by focused tests, both Clippy profiles and a release rebuild. Fixed-spawn High and Low gameplay captures were pixel-identical to baseline. No map, texture or renderer resource pipeline changed; no meaningful performance delta is claimed.

Native-input limitation: the computer-use tool sometimes sent SDL stale pointer coordinates, including a mouse-up at the previous physical location. Successful native clicks exercised navigation and value changes, but physical hover, held-pointer, outside-click, pause mouse-look and fullscreen pointer checks remain incomplete. Deterministic hit/edge tests passed; they do not replace those native checks. Alternate macOS input authorization was requested and remains pending.

Raw logs, traces, isolated settings and fixtures remain under `tools/bench/results/menus-20261011/` (ignored). No user settings or repository maps were modified. Exact-commit CI and custody are recorded in its final handoff.
