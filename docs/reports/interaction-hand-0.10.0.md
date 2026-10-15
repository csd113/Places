# Interaction hand

The interaction affordance now recreates the rounded off-white outline in the
user's Desktop reference, with a transparent palm/exterior and the current
interaction key inside. It occupies a fixed 16×16 reference-space quad low in
the HUD (about 30 pixels of visible ink at 960×544). There is no badge or extra
gameplay text. Existing precise targeting, pickup/drop/throw, held-item hiding,
menu/console behavior and flashlight logic are unchanged.

The committed `assets/shared/ui/font_01.png` is now 256×128, retaining the 2:1
aspect, original glyph pixels/orientation and white cell. Shared UV accessors
account for the new dimensions. A padded 64×64 hand cell uses spare space.
The HUD still uploads once, uses clamped nearest sampling without mipmaps,
and submits one draw; world texture quality never resizes this resource.
Decoded/uploaded storage increases by 96 KiB to 128 KiB. A one-character hand
uses 36 vertices instead of 1,014. No frame-rate improvement is claimed.

The catalog is byte-identical to the flashlight baseline. Existing package
discovery pins the entire catalog hash; keeping the existing registered PNG
avoids invalidating maps for this HUD edit. All eight shipped packages still
pass discovery. No map was baked or modified.

## Native acceptance

Apple M2 Pro / Metal, actual presented captures. The preserved current-model
item fixture is `local/model-integration-0.10.0/followup-item-physics.placesmap`
(SHA256 `eee02d53d123c592d10f18ac4333ee344277eb0c1dbc8ba78aa067df56bc5a4c`).
Player SHA256: `ae6d6c9686653481f15b984f813a7092174a173ffca2b23b88b3bdd1890af9cc`.

- Workflow: 56 checks pass, including idle, direct aim, pickup disappearance,
  real drop/throw, live Controls rebinding E→R, old E becoming inactive,
  menus, console and real light switching.
- Targeting: 498 checks pass, covering narrow off-axis misses, reach boundaries,
  occlusion, nearby objects, switches and doors.
- Lit-door fullscreen: 10 checks pass at 3024×1898. Other reviewed drawables:
  960×544, 1280×720, 1600×1600 and 2560×1440.
- Font baseline/candidate: 31 native checks each. Thirteen exact title/subtitle
  ink-mask comparisons show zero differences against the preserved flashlight
  player, across Low/Medium/High and the four windowed sizes. All 95 source
  glyphs, UV mapping, transparency and the small hand/key layout are also
  protected by bounded CPU tests; no new map/GPU dependency enters Rust tests.

Curated unmodified native captures:
[aim](../images/interaction-hand-0.10.0/01-aim.png),
[after pickup](../images/interaction-hand-0.10.0/02-after-pickup.png),
[dark/rebound](../images/interaction-hand-0.10.0/03-dark-rebound.png),
[bright door](../images/interaction-hand-0.10.0/04-bright-door.png),
[960×544](../images/interaction-hand-0.10.0/05-reference-size.png).

Source receipts and reproducible native plans remain under
`local/interaction-hand-0.10.0/`. Source gates cover formatting, all-target /
all-feature check, strict debug/release Clippy, all-feature/default workspace
tests, asset/texture validation and source Python tests.

The local opaque 450×1114 reference was viewed and hash-verified as
`6901637caf3b8791f7d3546ceee9f8543c136759544ff62b6d35e9bba12467e0`.
Built-in imagegen recreated and cleaned its outline; offline atlas packing
preserved original font bytes. The prompt required the reference's exact
finger/thumb/wrist silhouette, off-white ink, transparent palm/outside, thin
dark edge and no baked letter, badge or captions. This is recreated artwork,
not a pixel extraction. Long binding names shrink inside the fixed palm;
native key coverage used E/R, with BACKSPACE fit covered by the CPU fixture.
Native evidence is limited to this Mac; other hardware was not visually tested.
