# Whole-catalog model shadow review, 7 November 2026

## Result

Preserve the approved assets and current default. No additional confirmed
current-content shadow defect justified a renderer, material or model change.
Missing realtime body self-shadow is intentional, not a failed asset. The
[authoring convention](../assets/SHADOWS.md) now makes that choice explicit.

The [machine-readable inventory](../assets/shadow-inventory-2026-11-07.json)
accounts for all **426 GLB aliases / 417 unique GLBs**, with source hashes,
materials/rigs/clips, default/opt-in paths, floor/cast applicability, evidence and
action. All asset GLBs are registered. **415 models received two native views
each**, individually staged rather than competing for eight contact slots.
Spooner-Man and Sheet Ghost Cat remain present but disposition-only/deferred.
No Drift Lung GLB exists; nothing was deleted or integrated for it.

| Current model class | Unique GLBs |
| --- | ---: |
| Rigid opaque | 384 |
| Rigid mixed BLEND/opaque | 11 |
| Rigid MASK | 2 |
| Node-animated opaque | 5 |
| Skinned opaque | 13 |
| Skinned BLEND, including deferred cat | 2 |

## Visual and grounding evidence

Current player/compiler runtime source is `bb69702edac3d8f696e2821391d917211a35be4e`.
The preserved all-feature ARM64 player SHA256 is
`b6acb0b7071478f019a6e9a1cfaf03e211684df2ca56a36da4ec53ed6c886021`.
No executable, GLB, PNG, production map or renderer source was changed here.
High textures, Full lighting, bloom on, reflections off, physical 1280×720 were
used for the catalog views. The existing `lighting_contact_walls.json` fixture
was copied unchanged and compiled privately, with one normalized subject at a time.

- All **830 catalog captures** were visually reviewed. No missing model,
  accepted placeholder, gross detached footprint or new stuck-on body shading
  was found. Full images resolved ambiguous thumbnails; foam also received a
  natural-scale section view. Thin/single-sided trim remains intentional geometry.
- Native diagnostics separately validate all 830 captures: the **414 non-BLEND
  subjects** each admit one footprint with center X/Z under the staged bounds,
  floor Y=0, bottom gap 0.015 m, grounding factor 0.97 and upward projection.
  The wholly BLEND Sheet Ghost intentionally admits zero. This is admission and
  support/projection evidence, not a claim of mesh-silhouette shadows.
- Sixty Full/Static controls cover rat, chair, mixed glass, MASK grass and BLEND
  ghost: absent, grounded, moved/rotated, raised, offscreen/re-entry and restored.
  Counts are `[0,1,1,0,1,0]` for the four eligible classes and all zero for ghost.
  Empty/restored frames are pixel-exact. A fixed lower-floor RGB region changes
  by up to 2–8/255 in the grounded controls, and is pixel-exact to empty when
  raised. The ghost's same region stays exact throughout. The rotated grass can
  enter that region, so its moved-frame delta is not a pure shadow measurement.
  Small quantization-level bright differences are retained in the raw results.
- Additional native controls inspect all 11 installed fixture variants from
  underneath, all three generated door kinds from both sides at closed/open
  angles **0/90/90/0**, the automatically created washer drum, four representative
  static prop classes under Full/Static, and assembled elevator landing/cabin
  buttons. Existing compiler-geometry architecture acceptance is reused unchanged.

The ordinary floor shadow is subtle enough that many individual textured-floor
stills cannot positively certify it by eye alone. The per-capture diagnostics,
empty/raised controls and RGB differences corroborate that observation; the
report does not relabel an invisible-looking blob as a silhouette cast shadow.
Mounted lamps, roofs, trim, shore foam, floating creatures and glass were not
declared defective merely because their artificial ground-stage has different
applicability from an installed scene.

## Representative performance, not a per-model campaign

Same current executable/package and normalized one-subject view, Apple M2 Pro /
Metal, physical 1920×1080, High textures / Full lighting, bloom and requested
Full reflections (no active mirror). Actual **Immediate** after startup, V-sync
and all limiters off; no missed presentations, capture or concurrent build/image
review during timing. Two 1,200-frame runs per mode/class after 240 warmup frames,
off/on then on/off. All runs exceed display refresh substantially, not a refresh
plateau. The setup-only spawn causes the harness's diagnostic label; no draws,
presentation or effects are disabled. These are bounded setting controls, not a
new optimization or guaranteed gameplay FPS.

| Class | Default FPS | Realtime opt-in FPS | Default / opt-in mean loop ms |
| --- | ---: | ---: | ---: |
| Rigid concave chair | 666.6 | 675.0 | 1.500 / 1.482 |
| Mixed glass sculpture | 650.4 | 652.6 | 1.537 / 1.532 |
| MASK grass | 558.9 | 556.0 | 1.789 / 1.799 |
| Node-animated balloon | 734.5 | 729.7 | 1.362 / 1.370 |
| BLEND ghost | 534.1 | 537.3 | 1.872 / 1.861 |
| Opaque mannequin | 693.9 | 656.4 | 1.444 / 1.524 |
| Opaque rat | 716.9 | 363.5 | 1.395 / 2.751 |

The rat's default is about 97.2% higher FPS / 49.3% lower mean loop interval;
mannequin about 5.7% higher FPS. The separate-path classes vary within 1.3%, not
evidence of meaningful gains. Rat p99 is 2.714–3.064 ms default versus
6.419–7.738 ms opt-in; all raw means/tails/RSS and run identities are preserved.
Low polygon count alone does not establish that opaque posed self-shadowing is
cheap. Earlier Tall Guest results at `98aed14d` remain distinct evidence:
close 96.7→424.9 FPS and moving 177.7→426.4, with the documented appearance
tradeoff and independent grounding cost. They were not remeasured here.

## Limits and preservation

This is complete model-identity coverage, **not every placed instance, mounting,
pose, attachment or gameplay event**. All nine Tall Guest clips, every mannequin
pose, physical drops/water bobbing, ragdoll events, player-form camera transitions,
all scripted spawn templates and elevator travel/opening were not rerun. Underlying
nondeferred assets were viewed and shared paths inspected; the inventory names
those concrete runtime-class gaps. Transparent effects/water/blood/decals and
non-rendering triggers/lights/volumes have explicit applicability, not forced
opaque-body shadows. No Windows/Linux native or new device-loss/bind-group campaign
was run; unchanged settings/reflection/attachment acceptance at `98aed14d` and
visibility acceptance at `aac1c97a` are reused, not represented as fresh tests.

Known source limits remain deliberate: bind-bounds ordinary character contacts,
whole-bounds mixed-alpha contacts, eight-slot selection, no arbitrary tabletop or
water receivers. Hypothetical partially faded opaque actors retain full contact
strength until hidden, but no affected current project placement was found;
existing fade fixtures use all-BLEND ghosts. No speculative fix or mass bake.

The failed initial empty-despawn pilot and unsupported mechanism-ID survey are
preserved, replaced with successful bounded controls. The original survey's later
mechanism failure does not become a successful process receipt; its earlier rigid
subjects were rerun successfully. Four mechanism GLBs use the supported raw model
path for isolated inspection. Initial door controls correctly stopped at 28°
against the default player's body; clear-player controls reached 90°. Dark/screened
elevator views were not accepted as appearance coverage. A proposed room crossing
the shaft was correctly refused; a valid private source supplies the accepted
assembly view. These are harness/fixture corrections, not engine defects fixed.

All 417 GLB hashes remain unchanged. Ten private packages pass production
`verify --require-current`. The accepted executable and its prior complete Rust /
lint/build gates are reused because the committed change is documentation/data
only; no new Rust tests or rebuild were needed. No full shipped-map bake or CI wait.
The exact model identities, notably Tall Guest's hash and 5,138-vertex mapping,
must be refreshed/revalidated when a later export replaces them.

Evidence and runnable fixtures/scripts:
`/Users/connordawkins/Documents/Codex/2026-11-07/entity-shadow-review/`.
Start with `evidence/{survey-validation,floor-control-validation,timing-results,
package-source-validation}.json`, `native/`, `generated/`, and
`IMPLEMENTATION_HANDOFF.md`. Raw native capture timings are not FPS evidence.
