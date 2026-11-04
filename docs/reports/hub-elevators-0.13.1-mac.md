# Quiet Collection hub elevators — Mac acceptance, 2026-11-04

Collection 1.0.4 replaces the eight hub travel doors and eight return doors
with the current passenger elevators. Each hub cabin has one destination;
return travel arrives in that destination's matching signed hub bay.

The hub retains destination materials and preview props. External side shafts
keep cabins clear of existing room floors/ceilings. Home's final hall metre
opens into a call-button alcove. Office's cabin sits at Y=7.2 in the upper
entry foyer; its three-floor ramps and paper route remain intact. Outdoor
return booths now fit full-depth cabins, and Hollow's boundary fence joins
the enclosed shaft. Three narrow ground recesses prevent snow/asphalt/sand
from sharing the cabin floor plane. Office skirting stops at the clear doorway. Signs avoid
floor indicators, and F1/host guidance names the elevators.

All nine levels retain byte-equivalent parsed rooms, props (including every
quest actor/component/binding), existing regions, volumes, timers, sequences, script
declarations, ramps, routes, water, ladders and allowed transitions. Every
Luau module retains identical non-string code; only guidance text changed.
The reward door, ledger and CLEAR mechanism retain their existing behavior.

## Validation

Base: `3145c841` on `0.13.1-dev`, including the restored walnut surround,
narrow metal frame and loading upload batching. Work uses an isolated checkout and build target.
The direct release binaries use Rust/Cargo 1.99.1; no Mac app bundle is used.

- `cargo build --locked --release --workspace --all-features`: passed (offline,
  with the local compiler wrapper disabled).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- Asset/source validation passed with 17 warnings for intentionally external
  shaft walls outside ordinary room footprints.
- The engine geometry checker reports zero confirmed errors in all nine maps.
  Existing heuristic findings and conservative shell/trim face, material-corner
  and elevator-floor warnings remain; this is not a zero-warning claim.
- A complete Static-only project export was validated against all dependencies
  and exact source currency. Ground corrections rebuilt the three affected outdoor members; the final
  roof adjustment rebuilt only Home. Unchanged member records were reused. No Full lightmap solve was performed.
- Direct native editor captures review all nine maps and the sign/control fit.
  Direct game captures inspect the cabin, threshold, arrival and hall views.

`tools/bench/collection_elevators.py` is explicit developer acceptance, outside
CI. It uses staged initial approach poses, then ordinary movement and aimed
controls to board, load, walk out, return and leave the matching hub cabin.
Presented-frame snapshots avoid stale interaction picks after PNG readback.
Each run owns disposable settings, installed package and campaign fixtures.
The campaign checks closed-door collision, both hall controls, both normal
button-triggered loads, exact cabin-relative rider offsets, identical retained
loading frames, script diagnostics, saved-favor ledger/lock continuity and
native audio errors/drops/peak amplitude. A controlled 500 ms preparation delay
makes loading continuity observable; these runs are not timing benchmarks.

## Evidence and limits

Initial task evidence is retained under `local/hub-elevators/`, including native
traces, PNGs, captured game audio, per-route `passed.json`, build/lint logs,
source invariants and package/binary identities. The accepted playable export
is `local/hub-frame-refresh/environments-current-1.0.4.placesmap`; it is a development
validation package. Final release preparation retains the normal lighting
policy. Package files remain untracked under the repository's existing policy.

This acceptance covers Mac Metal and Static lighting. Full lighting and other
platforms were not exercised. All favor chains were preserved in sources;
this task does not claim eight new end-to-end quest completions. The existing
`collection-quest-check` walked Home but its Hollow case stopped with `Missing
quest actor`: that diagnostic still uses the obsolete `hollow_missing_friend`
id, while the edited map uses `editor_prop_202`. It was left outside this map
integration change; the actual Hollow elevator route and its saved favor state
are covered by the native campaign.

## Accepted outcome

All eight destination round trips passed with the two-favor campaign fixture
(Pool and Winter), preserving the ledger and locked reward door. A separate
Home round trip with all eight favors passed and returned to the reward door
unlocked and fully open. Every route checks normal boarding/exiting and both
load directions; the final Winter rerun explicitly raised the native window
after an earlier run lost focus and correctly paused simulation.

All nine exported members match their current sources and intact dependencies.
The final asset refresh prepares all nine members against the corrected cabin.
Collection 1.0.4 can replace the earlier local 1.0.3 export without an equal-version content conflict. The binary SHA-256 is
`4156a1806f9e16159ba9d4d880391420447b668ed76fc48d8d2da667f9dbea8a`. Final development package SHA-256:
`3e7937ae9a75137574ada1bd8985527d7b74f3bc297573df152ba70ae7ce4a39`.

The refreshed aggregate receipt and eight-view contact sheet are retained under
`local/hub-frame-refresh/`. The original receipts remain available separately.


## Corrected frame integration

The local hub commit was rebased onto `3145c841` without conflicts. The corrected
cabin keeps walnut on the surrounding front wall and above the door, with metal
limited to the narrow frame/header and exposed pocket returns. Bounds, collision,
door movement and transport code are unchanged. Nine nearest-surface wall probes
find walnut, fifteen frame probes find metal, and all fourteen exposed return
triangles remain metal. The actual GLB seal, UV and surface-join checks pass.
The release build and strict all-target/all-feature Clippy pass again. This
refresh uses only Static preparation and isolated direct-binary playtests.

Corrected cabin SHA-256:
`ba51fdca4c0d1efefbd9d052e62b397337ecf0da00acebac7a30022906d16de2`.

The refreshed 1.0.4 package passed all eight direct-binary round trips with
partial saves and a separate all-favors Home round trip. All nine members
verify current. The final receipt is `local/hub-frame-refresh/receipt.json`;
`native-return-cabins.png` and `visual/state/` retain the current game views.
Native audio reported no errors or dropped voices; observed peak was
0.1343. No Full lightmap solve was performed.
