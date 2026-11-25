# Mannequin Ragdoll Lab

Open this folder in `places-editor`, Save+Build and Play. This is an ordinary
editable project using installed models, routes, stairs, props, trigger bindings
and sandboxed Luau. It does not change any other map or the menu cast.

Enable Developer Console in Settings and open it with backtick. The wandering
mannequin at the starting view is `floor_actor`:

```
/ragdoll floor_actor on 1.5 0 -0.4
/ragdoll floor_actor reset
```

The optional three numbers are world velocity in metres/second, capped at 15.
Displayed animation and movement velocities are inherited too. Reset restores
the authored placement, animation clock and route/AI behavior. Reload restores
the whole authored world. There is no automatic stand-up animation.

Use `/trigger drop_wall`, `/trigger drop_stair` and `/trigger drop_prop` for
wall, descending stair and coffee-table contact. `/trigger script_demo` uses
the Luau API. `/trigger drop_all` activates eight mannequins; `/trigger reset_all`
removes their bodies/joints and restores their authored behavior. The four extra
actors are named `extra_1` through `extra_4`; the last has an ordinary AI controller.
The normal E interaction on each mannequin also activates its authored trial.

For socket custody: pick up the cushion near the starting actor, aim at that
actor within normal handoff reach, then `/trigger carry_demo`. The ordinary `handoff` action
attaches the same item to the posed hand. Activating the ragdoll returns that
item to physics at the current socket before the actor falls. Individual actor
reset leaves the released item physical; map reload restores the item too.

JSON bindings use `{"action":"ragdoll","target":"floor_actor","enabled":true,
"velocity":[1.5,0,-0.4]}`; `enabled:false` resets. Luau uses
`api.ragdoll("floor_actor", true, 1.5, 0, -0.4)` and `api.ragdoll("floor_actor", false)`.
Unsupported rigs are refused with status/log diagnostics and retain animation.

The shared world permits eight ragdolls and 128 total item/bone bodies. Each
mannequin owns fifteen bodies and fourteen limited joints. Connected segments
have no internal collision; other mannequins, props, world geometry, doors and
the player proxy use normal contacts. Hands carry their finger descendants.
Pause, time scale and single-step use the existing physics clock.

Export this project from the editor and play it in the native player for explicit
checks of contacts, carry, stress, lifecycle and the unsupported-body case.
Ordinary Cargo tests and CI do not bake this map. For throughput comparisons,
disable V-sync and every frame limiter, record actual presentation mode and
drawable size, and treat a refresh-rate plateau as inconclusive. Record video
separately from timing runs.
