# Developer console — 0.10.0 staging

Enable **Settings → Controls → Developer Console** (Off by default). During
play, press **backtick** to open or close it; **Escape** closes it without opening
the pause menu. The world pauses while typing. The cursor is released, SDL text
input owns the keyboard, and closing clears held controls and resets timing.
Focus loss, menus and loading close the console. The setting is persisted;
commands, history and previews are session-local.

| Command | Current behavior |
| --- | --- |
| `/help` | Lists the typed command registry and controls. |
| `/clear` | Clears output; command history remains. |
| `/noclip [on\|off]` | Toggles, or explicitly selects, collision-free flight. Normal movement/look bindings apply; hold Jump to rise and Crouch to descend. |
| `/give <catalog-id>` | Places a small rigid **visual prop preview** on clear flat ground ahead. No inventory, pickup, collision or physics is claimed. |

Up/Down recall history and restore an unfinished draft; PageUp/PageDown or the
mouse wheel scroll wrapped output. Text editing appends and backspaces at the
end of the line. Storage is bounded to 256 ASCII input bytes, 32 commands and
128 output messages of at most 512 characters. Only changed console UI geometry
is rebuilt, through the existing font/quad pipeline. No dependency was added.

Noclip has a visible state in the console and an indicator during flight. Exit
checks the full standing body against solids, live doors and ceilings, and the
existing geometry-derived playable space. Clear in-map positions are retained;
otherwise a revalidated entry pose or validated spawn is used. Motion, stance,
water/ladder state, camera pose and trigger membership are reconciled without
resetting world entities. If a malformed world offers no safe destination,
exit is refused with an error; disabling the setting is also refused until the
player moves clear. Map/player reset clears the mode. Ordinary walking and void
recovery keep their existing paths.

`/give core:rubber_duck` is an example. Exact catalog IDs with a rigid model,
positive actual model bounds and no axis over 1.5 m are eligible. Characters,
morph models, materials/textures, missing models, raw paths and unknown IDs are
refused. Placement uses the player's current storey, flat footprint support,
conservative ceiling/solid/live-door clearance and non-overlap with other
previews. Noclip placement is refused. At most 16 previews exist per world;
disabling the console or installing/reloading world resources removes them.
Models, decoded textures and dynamic meshes reuse the existing renderer caches.
The first use of an uncached model still loads/uploads it synchronously.

`src/console/command.rs` owns the closed typed registry/parser. Console state,
UI, SDL ownership and runtime dispatch have focused modules. `CommandTarget`
is the small integration point for subsequent gameplay systems; today's live
target intentionally gives visual props. `src/game/developer.rs` owns flight
and placement validation. Console input never executes OS commands, shell
expressions, scripts or arbitrary file paths.

## Validation

Formatting and strict `cargo clippy --workspace --all-targets --all-features --
-D warnings` passed. `cargo test --workspace` passed (2,269 library tests);
`cargo test --workspace --all-features` passed (2,275). Binary, integration and
doc-test targets also passed; 19 existing opt-in GPU tests remain ignored.
The four new deterministic tests cover parser failures, enable gating, bounded
input/history/output, draft recall, dispatcher results, safe flight/exit/reset
and current-storey placement. They build only a tiny two-room fixture, need no
GPU and finish in less than 0.01 s. No production map bake was added to tests
or CI. Python tooling compiled successfully.

Seven sequential native acceptance runs used the existing Office and Arcade
packages through SDL/Metal on this Mac. Scripted key/text events exercised the
ordinary event handler; manual held-key acceptance and other platforms are not
claimed. Actual receipts assert menu persistence, disabled gating, zero held
controls while typing, released/restored mouse capture, history/output overflow
(70 submissions), help/clear, visible rigid props, invalid/non-placeable/character
IDs, duplicate placement refusal, two shared-model instances and cleanup.
Focus loss and Office → Arcade → menu → Office transitions close the console
and reset flight on world replacement. Noclip exits from ceiling overlap and
beyond every Office room immediately return to the validated entry; a clear
in-map exit retains its position. Screenshots were inspected for readability.

| Native timing mode | Loop median (ms) | Loop p95 (ms) | Update median (ms) |
| --- | ---: | ---: | ---: |
| Preserved lighting build | 1.902 | 2.506 | 1.202 |
| Console disabled | 1.930 | 2.948 | 1.199 |
| Help overlay open; world paused | 1.339 | 2.716 | 0.432 |

Values are means of two run medians/p95s, 600 measured frames per run after
120 warmup frames, with reversed order. Office, fixed camera, High textures,
Full lighting/reflections, Immediate presentation and 2560×1440 drawable were
matched; builds were idle. Disabled median loop difference is +0.028 ms; the
update median did not increase. Tail results varied and do not establish a
general tail guarantee. Open-console timings describe a paused world and
cannot be treated as a gameplay speedup. Raw receipts and the preserved
executable live under `local/developer-console-0.10.0/`.

Reproduce explicitly with an existing package:

```sh
python3 tools/bench/developer_console.py \
  --binary local/developer-console-0.10.0/bin/places \
  --mode input --out local/my-console-input
```

Other acceptance modes: `settings`, `preview`, `recovery`, `overflow`,
`transitions`. `tools/bench/frame_time.py --console-open` measures the open
help overlay. Neither tool bakes maps.

| Enable setting | Help and input | Actual prop preview | Safe return |
| --- | --- | --- | --- |
| ![Setting](../images/reports/developer-console-0.10.0/setting.png) | ![Console](../images/reports/developer-console-0.10.0/console.png) | ![Given prop](../images/reports/developer-console-0.10.0/give.png) | ![Recovered player](../images/reports/developer-console-0.10.0/recovery.png) |

Existing map packages, assets, archived historical JSON, lighting settings and
performance fixes were preserved. Physics, inventory, audio and flashlight
systems were not implemented. The prior intermittent gameplay crash remains
unexplained; this task does not claim to solve it.
