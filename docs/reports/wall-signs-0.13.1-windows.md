# Hub wall-sign authoring â€” Windows, 0.13.1 development

The editor now authors the hub's existing file-backed `decals[]` signs. No
parallel prop, text renderer, generated bitmap, lighting source or package
format was introduced. The ten committed 1024Ã—256 PNGs own their title and
subtitle. The UI selects printed artwork; arbitrary typed sign lettering is
outside the current decal contract.

## Authoring and geometry

- Simple mode exposes **Add props / signs > â†’ Add Wall Sign**. The same action
  is in Advanced's scene-object creation sheet. Click a rectangular wall's
  broad face; the preview is private until clicked. Escape/focus loss cancels.
- New and existing vertical decals share selection/picking, move axes,
  transforms, copy/delete, history, persistent locks and layer focus. Wall
  edits retain existing attached-decal behavior, including locked attachments.
- The sign sheet selects existing artwork, preserves 4:1 size through normal
  size controls, and exposes cardinal facing and explicit mounting. Pose
  supports exact centre, fitted width/height and in-plane spin.
- Mounting fits the entire rotated footprint to solid wall coverage, including
  continuous lintels spanning opening slices. Source coordinates lie on the
  actual carrier; the existing renderer supplies its 0.2 mm lift and biased
  decal pass exactly once. Repeated mounting does not accumulate offsets.
- Copies find disjoint supported space on the same wall plane. If no clear
  area fits, the operation is refused without changing history. Repeated Add
  clicks also cannot stack overlapping decals. Manual transforms remain
  explicit author control.
- Hub source centres lost their redundant 1 mm offsets. CENTRAL HUB was raised
  from 2.4 to 2.9 m so its 1 m plate clears the 2.4 m doorway. Artwork, routes,
  props, lighting and the rest of the hub were retained.

## Validation receipts

Evidence root:
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-29/task-4/`.

- Rust 1.99.1 / Cargo 1.99.1. Full locked production build, format, all-target
  all-feature check, strict debug/release Clippy and workspace tests are recorded
  in `gates-rust-final/` and `gates-rust-tests-final/` (392 passed).
  `acceptance.json` indexes the completed gates and source/binary hashes.
- `gates-source/` records all asset/generator checks and 371 Python contracts
  (18 optional native suites skipped). Asset validation: 674 assets, zero
  warnings. Texture check passes with 112 existing manifest advisories.
- The new compiler checked every environment-project source without baking.
  Two small Rust regressions cover cardinal wall support/opening/offset rules,
  repeated placement/copy overlap and document history/lock/source preservation.
  They do not bake maps or require a GPU.
- Twenty-eight final native state assertions passed. Native recipes exercise
  cancellation, repeated operations, printed artwork,
  numeric empty-input refusal, long/empty/special-character scene search,
  copy/delete/history, locks, facing, layer isolation, save/reopen and Play.
  Display scale was 1.5, pixel density 1.0; actual windows/drawables were
  960Ã—544, 1000Ã—900, 1440Ã—900 and 1920Ã—1080. Modal artwork controls remain present.
- `package-proof/` validates the tiny authored bundle, verifies compiler/source
  currency and recovers byte-exact editable source. Native player captures in
  Static and Full preserve the selected FRUTIGER AERO title and subtitle.
  The actual player was DX12 on the NVIDIA GeForce RTX 3070 Laptop GPU, with
  a 960×600 drawable and Fifo presentation; no throughput claim is made.
- The real hub was opened read-only in the editor and all ten signs selected;
  retained native views cover south/north/east/west normals. Existing scenery
  can occlude a camera angle; perspective and front views are preserved.
- Background builds/compiler work use BelowNormal priority. Native editor and
  player processes use Normal. These are functional checks, not FPS comparisons.

The initial authoring pilot ended without a final receipt. Its partial evidence
is retained and is not counted as a pass.
The fresh `authoring-r1` run completed. Final acceptance uses the final binaries
and separately retained `*-final` run receipts. The shared Luau check required
an explicit Windows `.exe` compiler path; that corrected command passed eleven
bounded refusals and the maintained environment-project check without a bake.

The OS-input sanity check sent Escape and Alt+F4 to the final editor and
confirmed normal shutdown. Windows Graphics Capture timed out twice and its
coordinate click path was unavailable. Visual review therefore uses the
application's native GPU captures; broad OS-driven pointer/drag acceptance is
not claimed. Only the observed 150% Windows display scale is claimed.

## Scope and handoff

Curved/angled/end-cap sign placement and arbitrary typed lettering are not
represented by this editor tool. Existing PNG lettering is unchanged, including
long titles. Native macOS acceptance is explicitly deferred. No production map
was rebaked; the installed production hub bundle therefore retains its prior
source until the planned packaging refresh after the remaining engine work. Only the tiny
acceptance project was exported and played.

Final binary/source identities, native assertions and clean-repository custody
are retained beside the evidence. No release, main merge, tag, force push,
security/power setting or iGPU change is part of this task.
